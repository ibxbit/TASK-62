/// Ops admin: Fare rule configuration.
///
/// Lists all fare rules (network-wide or per-route), allows creating and
/// deleting them.  Admin-only — guarded at the router level.
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::ops_service,
    types::ops::{CreateFareRuleRequest, FareRule, OpsRoute},
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<FareRule>), Error(String) }

#[derive(Clone, PartialEq)]
enum FormState { Hidden, Visible, Submitting, Failed(String) }

const RULE_TYPES: &[&str] = &["flat", "distance_based", "zone_based", "peak_surcharge"];

#[function_component(FareRulesPage)]
pub fn fare_rules_page() -> Html {
    let page_state   = use_state(|| PageState::Loading);
    let form_state   = use_state(|| FormState::Hidden);
    let routes       = use_state::<Vec<OpsRoute>, _>(Vec::new);
    let del_working  = use_state(|| false);

    // Form fields
    let route_sel  = use_state::<Option<Uuid>, _>(|| None);
    let type_input = use_state(|| RULE_TYPES[0].to_string());
    let fare_input = use_state(|| "0.00".to_string());

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match ops_service::list_fare_rules().await {
                    Ok(rules) => ps.set(PageState::Loaded(rules)),
                    Err(e)    => ps.set(PageState::Error(e)),
                }
            });
        }
    };

    // Load fare rules and routes on mount
    {
        let rel = reload.clone();
        let rts = routes.clone();
        use_effect_with((), move |_| {
            rel();
            let rts = rts.clone();
            spawn_local(async move {
                if let Ok(page) = ops_service::list_routes_admin().await {
                    rts.set(page.data);
                }
            });
            || ()
        });
    }

    let on_create = {
        let fs      = form_state.clone();
        let rsel    = route_sel.clone();
        let typ_inp = type_input.clone();
        let fare    = fare_input.clone();
        let rel     = reload.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let base_fare = match (*fare).parse::<f64>() {
                Ok(f) if f >= 0.0 => f,
                _ => {
                    fs.set(FormState::Failed("Base fare must be a non-negative number.".to_string()));
                    return;
                }
            };
            let body = CreateFareRuleRequest {
                route_id:  *rsel,
                rule_type: (*typ_inp).clone(),
                base_fare,
            };
            let fs2 = fs.clone();
            let rel = rel.clone();
            spawn_local(async move {
                fs2.set(FormState::Submitting);
                match ops_service::create_fare_rule(&body).await {
                    Ok(_)  => { fs2.set(FormState::Hidden); rel(); }
                    Err(e) => fs2.set(FormState::Failed(e)),
                }
            });
        })
    };

    let on_delete = {
        let dw  = del_working.clone();
        let rel = reload.clone();
        Callback::from(move |rule_id: Uuid| {
            let dw2 = dw.clone();
            let rel = rel.clone();
            spawn_local(async move {
                dw2.set(true);
                let _ = ops_service::delete_fare_rule(rule_id).await;
                dw2.set(false);
                rel();
            });
        })
    };

    // ── Route options for the form select ─────────────────────────────────────

    let route_options: Html = std::iter::once(html! {
        <option value="">{ "— All routes (network-wide) —" }</option>
    })
    .chain((*routes).iter().map(|r| {
        let id_str = r.id.to_string();
        let label  = format!("{} – {}", r.code, r.name);
        html! { <option value={id_str}>{ label }</option> }
    }))
    .collect();

    // ── Table content ─────────────────────────────────────────────────────────

    let content = match &*page_state {
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        PageState::Loaded(rules) if rules.is_empty() => html! {
            <div class="empty-state"><p>{ "No fare rules configured." }</p></div>
        },
        PageState::Loaded(rules) => {
            let del = on_delete.clone();
            let is_del = *del_working;
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Type" }</th>
                            <th>{ "Base Fare" }</th>
                            <th>{ "Route" }</th>
                            <th>{ "Active" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for rules.iter().map(|r| {
                            let rid  = r.id;
                            let del2 = del.clone();
                            let route_label = r.route_id
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "Network-wide".to_string());
                            html! {
                                <tr key={rid.to_string()}>
                                    <td class="mono">{ &r.rule_type }</td>
                                    <td>{ format!("{:.2}", r.base_fare) }</td>
                                    <td>{ route_label }</td>
                                    <td>{ if r.is_active { "Yes" } else { "No" } }</td>
                                    <td class="action-cell">
                                        <button class="btn btn--small btn--danger"
                                                disabled={is_del}
                                                onclick={Callback::from(move |_| del2.emit(rid))}>
                                            { "Delete" }
                                        </button>
                                    </td>
                                </tr>
                            }
                        }) }
                    </tbody>
                </table>
            }
        }
    };

    // ── Create form modal ─────────────────────────────────────────────────────

    let form_modal = match &*form_state {
        FormState::Visible | FormState::Submitting | FormState::Failed(_) => {
            let is_sub  = matches!(&*form_state, FormState::Submitting);
            let err_msg = if let FormState::Failed(e) = &*form_state {
                html! { <p class="form-error">{ e }</p> }
            } else { html! {} };
            let cancel = {
                let fs = form_state.clone();
                Callback::from(move |_: MouseEvent| fs.set(FormState::Hidden))
            };
            let rule_type_options: Html = RULE_TYPES.iter().map(|t| {
                html! { <option value={*t}>{ *t }</option> }
            }).collect();
            html! {
                <div class="modal-overlay">
                    <div class="modal">
                        <h2 class="modal__title">{ "Create Fare Rule" }</h2>
                        { err_msg }
                        <form onsubmit={on_create} class="modal__form">
                            <label class="form-field">
                                <span>{ "Rule type" }</span>
                                <select class="form-field__input"
                                        onchange={{
                                            let t = type_input.clone();
                                            Callback::from(move |e: Event| {
                                                let el: web_sys::HtmlSelectElement = e.target_unchecked_into();
                                                t.set(el.value());
                                            })
                                        }}>
                                    { rule_type_options }
                                </select>
                            </label>
                            <label class="form-field">
                                <span>{ "Base fare" }</span>
                                <input type="number" min="0" step="0.01"
                                       class="form-field__input"
                                       value={(*fare_input).clone()}
                                       oninput={{
                                           let f = fare_input.clone();
                                           Callback::from(move |e: InputEvent| {
                                               let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                               f.set(el.value());
                                           })
                                       }} />
                            </label>
                            <label class="form-field">
                                <span>{ "Route (optional)" }</span>
                                <select class="form-field__input"
                                        onchange={{
                                            let rs = route_sel.clone();
                                            Callback::from(move |e: Event| {
                                                let el: web_sys::HtmlSelectElement = e.target_unchecked_into();
                                                let val = el.value();
                                                rs.set(val.parse::<Uuid>().ok());
                                            })
                                        }}>
                                    { route_options }
                                </select>
                            </label>
                            <div class="form-actions">
                                <button type="submit" class="btn btn--primary"
                                        disabled={is_sub}>
                                    { if is_sub { "Creating…" } else { "Create Rule" } }
                                </button>
                                <button type="button" class="btn btn--secondary"
                                        onclick={cancel}>
                                    { "Cancel" }
                                </button>
                            </div>
                        </form>
                    </div>
                </div>
            }
        }
        FormState::Hidden => html! {}
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Fare Rules" }</h1>
                <div class="page__actions">
                    <button class="btn btn--primary"
                            onclick={Callback::from({
                                let fs = form_state.clone();
                                move |_| fs.set(FormState::Visible)
                            })}>
                        { "+ Create Rule" }
                    </button>
                </div>
            </header>
            <div class="page__body">
                { content }
            </div>
            { form_modal }
        </div>
    }
}
