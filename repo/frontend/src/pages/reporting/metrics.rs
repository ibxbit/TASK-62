/// Reporting: KPI metric definitions page.
///
/// Lists defined metrics, allows creating/deleting them.
/// Create and delete are reauth-gated (server returns 403 on stale session).
///
/// Clicking a metric row opens a drilldown panel where the user can specify a
/// date range and fetch computed values for that metric (with dimension breakdown).
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    components::reauth_prompt::ReauthPrompt,
    services::reporting_service,
    types::reporting::{CreateMetricRequest, MetricDefinition, MetricValue},
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<MetricDefinition>), Error(String) }

#[derive(Clone, PartialEq)]
enum ActionState { Idle, Working, ReauthRequired, Done(String), Failed(String) }

#[derive(Clone, PartialEq)]
enum DrilldownState { Idle, Loading, Loaded(Vec<MetricValue>), Error(String) }

#[function_component(MetricsPage)]
pub fn metrics_page() -> Html {
    let page_state      = use_state(|| PageState::Loading);
    let action_state    = use_state(|| ActionState::Idle);
    let key_input       = use_state(String::new);
    let name_input      = use_state(String::new);
    let pending_del     = use_state::<Option<uuid::Uuid>, _>(|| None);

    // Drilldown state
    let drilldown_metric = use_state::<Option<MetricDefinition>, _>(|| None);
    let drilldown_state  = use_state(|| DrilldownState::Idle);
    let drill_from       = use_state(|| "2025-01-01".to_string());
    let drill_to         = use_state(|| "2025-12-31".to_string());
    let drill_route_id   = use_state(String::new);
    let drill_depot_id   = use_state(String::new);

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match reporting_service::list_metrics().await {
                    Ok(items) => ps.set(PageState::Loaded(items)),
                    Err(e)    => ps.set(PageState::Error(e)),
                }
            });
        }
    };

    { let r = reload.clone(); use_effect_with((), move |_| { r(); || () }); }

    let create_metric = {
        let ast   = action_state.clone();
        let key   = key_input.clone();
        let name  = name_input.clone();
        let rel   = reload.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let body = CreateMetricRequest {
                metric_key:   (*key).clone(),
                display_name: (*name).clone(),
                description:  None,
                formula_type: "custom_sql".to_string(),
            };
            let ast2 = ast.clone();
            let rel2 = rel.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match reporting_service::create_metric(&body).await {
                    Ok(_)  => { ast2.set(ActionState::Done("Metric created".into())); rel2(); }
                    Err(e) if e.contains("[403]") => ast2.set(ActionState::ReauthRequired),
                    Err(e) => ast2.set(ActionState::Failed(e)),
                }
            });
        })
    };

    let delete_metric = {
        let ast  = action_state.clone();
        let pend = pending_del.clone();
        let rel  = reload.clone();
        Callback::from(move |mid: uuid::Uuid| {
            let ast2  = ast.clone();
            let rel2  = rel.clone();
            pend.set(Some(mid));
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match reporting_service::delete_metric(mid).await {
                    Ok(_)  => { ast2.set(ActionState::Done("Metric deleted".into())); rel2(); }
                    Err(e) if e.contains("[403]") => ast2.set(ActionState::ReauthRequired),
                    Err(e) => ast2.set(ActionState::Failed(e)),
                }
            });
        })
    };

    // ── Drilldown fetch ───────────────────────────────────────────────────────

    let fetch_drilldown = {
        let ds       = drilldown_state.clone();
        let dm       = drilldown_metric.clone();
        let from     = drill_from.clone();
        let to       = drill_to.clone();
        let route_id = drill_route_id.clone();
        let depot_id = drill_depot_id.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let metric_id = match (*dm).as_ref().map(|m| m.id) {
                Some(id) => id,
                None     => return,
            };
            let from_str  = (*from).clone();
            let to_str    = (*to).clone();
            let route_str = (*route_id).clone();
            let depot_str = (*depot_id).clone();
            let ds2       = ds.clone();
            spawn_local(async move {
                ds2.set(DrilldownState::Loading);
                let route_opt = if route_str.is_empty() { None } else { Some(route_str.as_str()) };
                let depot_opt = if depot_str.is_empty() { None } else { Some(depot_str.as_str()) };
                match reporting_service::get_metric_values(
                    metric_id, &from_str, &to_str, route_opt, depot_opt,
                ).await {
                    Ok(vals) => ds2.set(DrilldownState::Loaded(vals)),
                    Err(e)   => ds2.set(DrilldownState::Error(e)),
                }
            });
        })
    };

    // ── Metric table ──────────────────────────────────────────────────────────

    let content = match &*page_state {
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        PageState::Loaded(items) if items.is_empty() => html! {
            <div class="empty-state"><p>{ "No metrics defined." }</p></div>
        },
        PageState::Loaded(items) => {
            let del = delete_metric.clone();
            let dm  = drilldown_metric.clone();
            let dds = drilldown_state.clone();
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Key" }</th>
                            <th>{ "Display Name" }</th>
                            <th>{ "Formula Type" }</th>
                            <th>{ "Builtin" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for items.iter().map(|m| {
                            let mid     = m.id;
                            let del2    = del.clone();
                            let dm2     = dm.clone();
                            let dds2    = dds.clone();
                            let builtin = m.is_builtin;
                            let metric  = m.clone();
                            html! {
                                <tr key={mid.to_string()}
                                    class="data-table__row--clickable"
                                    onclick={Callback::from(move |_| {
                                        dm2.set(Some(metric.clone()));
                                        dds2.set(DrilldownState::Idle);
                                    })}>
                                    <td class="mono">{ &m.metric_key }</td>
                                    <td>{ &m.display_name }</td>
                                    <td>{ m.formula_label() }</td>
                                    <td>{ if builtin { "Yes" } else { "No" } }</td>
                                    <td class="action-cell">
                                        if !builtin {
                                            <button class="btn btn--small btn--danger"
                                                    onclick={Callback::from(move |e: MouseEvent| {
                                                        e.stop_propagation();
                                                        del2.emit(mid);
                                                    })}>
                                                { "Delete" }
                                            </button>
                                        }
                                    </td>
                                </tr>
                            }
                        }) }
                    </tbody>
                </table>
            }
        }
    };

    // ── Drilldown panel ───────────────────────────────────────────────────────

    let drilldown_panel = if let Some(metric) = &*drilldown_metric {
        let ds_body = match &*drilldown_state {
            DrilldownState::Idle    => html! { <p class="hint">{ "Set a date range and click Fetch." }</p> },
            DrilldownState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
            DrilldownState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
            DrilldownState::Loaded(vals) if vals.is_empty() => html! {
                <p class="empty-state__message">{ "No values in the selected range." }</p>
            },
            DrilldownState::Loaded(vals) => html! {
                <table class="data-table data-table--sm">
                    <thead>
                        <tr>
                            <th>{ "Period Start" }</th>
                            <th>{ "Period End" }</th>
                            <th>{ "Value" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for vals.iter().map(|v| html! {
                            <tr>
                                <td>{ v.period_start.format("%Y-%m-%d %H:%M").to_string() }</td>
                                <td>{ v.period_end.format("%Y-%m-%d %H:%M").to_string() }</td>
                                <td>{ format!("{:.4}", v.value) }</td>
                            </tr>
                        }) }
                    </tbody>
                </table>
            },
        };

        let close_drill = {
            let dm = drilldown_metric.clone();
            Callback::from(move |_: MouseEvent| dm.set(None))
        };

        html! {
            <div class="drilldown-panel">
                <div class="drilldown-panel__header">
                    <h2>{ format!("Drilldown: {}", metric.display_name) }</h2>
                    <button class="btn btn--secondary btn--small" onclick={close_drill}>
                        { "Close" }
                    </button>
                </div>
                <form onsubmit={fetch_drilldown} class="inline-form">
                    <label class="form-field form-field--inline">
                        <span>{ "From (date)" }</span>
                        <input type="date" class="form-field__input"
                               value={(*drill_from).clone()}
                               oninput={{
                                   let f = drill_from.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       f.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field form-field--inline">
                        <span>{ "To (date)" }</span>
                        <input type="date" class="form-field__input"
                               value={(*drill_to).clone()}
                               oninput={{
                                   let t = drill_to.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       t.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field form-field--inline">
                        <span>{ "Route ID (optional)" }</span>
                        <input type="text" class="form-field__input"
                               placeholder="UUID or leave blank"
                               value={(*drill_route_id).clone()}
                               oninput={{
                                   let r = drill_route_id.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       r.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field form-field--inline">
                        <span>{ "Depot ID (optional)" }</span>
                        <input type="text" class="form-field__input"
                               placeholder="UUID or leave blank"
                               value={(*drill_depot_id).clone()}
                               oninput={{
                                   let d = drill_depot_id.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       d.set(el.value());
                                   })
                               }} />
                    </label>
                    <button type="submit" class="btn btn--primary">{ "Fetch Values" }</button>
                </form>
                <div class="drilldown-panel__body">{ ds_body }</div>
            </div>
        }
    } else {
        html! {}
    };

    // ── Action feedback ───────────────────────────────────────────────────────

    let feedback = match &*action_state {
        ActionState::Working   => html! { <div class="action-feedback action-feedback--working">{ "Working…" }</div> },
        ActionState::Done(msg) => html! { <div class="action-feedback action-feedback--success">{ msg }</div> },
        ActionState::Failed(e) => html! { <div class="action-feedback action-feedback--error">{ e }</div> },
        _ => html! {},
    };

    let reauth_overlay = if *action_state == ActionState::ReauthRequired {
        let pend = *pending_del;
        let ast  = action_state.clone();
        let del  = delete_metric.clone();
        html! {
            <ReauthPrompt
                on_success={Callback::from(move |_| {
                    if let Some(mid) = pend {
                        del.emit(mid);
                    } else {
                        ast.set(ActionState::Idle);
                    }
                })}
                on_cancel={Callback::from({
                    let ast = action_state.clone();
                    move |_| ast.set(ActionState::Idle)
                })}
            />
        }
    } else { html! {} };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "KPI Metrics" }</h1>
            </header>
            <div class="page__body">
                <form onsubmit={create_metric} class="inline-form">
                    <label class="form-field form-field--inline">
                        <span>{ "Key" }</span>
                        <input type="text" placeholder="e.g. on_time_rate"
                               oninput={{
                                   let k = key_input.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       k.set(el.value());
                                   })
                               }}
                               class="form-field__input" />
                    </label>
                    <label class="form-field form-field--inline">
                        <span>{ "Display Name" }</span>
                        <input type="text" placeholder="On-Time Rate"
                               oninput={{
                                   let n = name_input.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       n.set(el.value());
                                   })
                               }}
                               class="form-field__input" />
                    </label>
                    <button type="submit" class="btn btn--primary">{ "Create Metric" }</button>
                </form>
                { feedback }
                { content }
                { drilldown_panel }
            </div>
            { reauth_overlay }
        </div>
    }
}
