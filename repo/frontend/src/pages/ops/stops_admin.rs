/// Ops admin: Stop management for a selected route.
///
/// The user first selects a route from the dropdown; then the stops for that
/// route are loaded and displayed with CRUD actions (add / delete).
/// Admin-only — guarded at the router level.
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::ops_service,
    types::ops::{CreateOpsStopRequest, OpsRoute, OpsStop},
};

#[derive(Clone, PartialEq)]
enum PageState { Init, Loading, Loaded(Vec<OpsStop>), Error(String) }

#[derive(Clone, PartialEq)]
enum FormState { Hidden, Visible, Submitting, Failed(String) }

#[function_component(StopsAdminPage)]
pub fn stops_admin_page() -> Html {
    let routes       = use_state::<Vec<OpsRoute>, _>(Vec::new);
    let sel_route_id = use_state::<Option<Uuid>, _>(|| None);
    let page_state   = use_state(|| PageState::Init);
    let form_state   = use_state(|| FormState::Hidden);
    let del_working  = use_state(|| false);

    // Form fields
    let code_input = use_state(String::new);
    let name_input = use_state(String::new);
    let seq_input  = use_state(|| "1".to_string());

    // Load route list on mount
    {
        let rts = routes.clone();
        let ps  = page_state.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match ops_service::list_routes_admin().await {
                    Ok(page) => rts.set(page.data),
                    Err(e)   => ps.set(PageState::Error(e)),
                }
            });
            || ()
        });
    }

    // Load stops for the selected route
    let load_stops = {
        let ps = page_state.clone();
        Callback::from(move |route_id: Uuid| {
            let ps = ps.clone();
            spawn_local(async move {
                ps.set(PageState::Loading);
                match ops_service::list_stops(route_id).await {
                    Ok(stops) => ps.set(PageState::Loaded(stops)),
                    Err(e)    => ps.set(PageState::Error(e)),
                }
            });
        })
    };

    let on_route_select = {
        let sel  = sel_route_id.clone();
        let ps   = page_state.clone();
        let load = load_stops.clone();
        Callback::from(move |e: Event| {
            let el: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let val = el.value();
            if val.is_empty() {
                sel.set(None);
                ps.set(PageState::Init);
            } else if let Ok(id) = val.parse::<Uuid>() {
                sel.set(Some(id));
                load.emit(id);
            }
        })
    };

    let on_create = {
        let fs   = form_state.clone();
        let code = code_input.clone();
        let name = name_input.clone();
        let seq  = seq_input.clone();
        let sel  = sel_route_id.clone();
        let load = load_stops.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let route_id = match *sel {
                Some(id) => id,
                None     => return,
            };
            if (*code).trim().is_empty() || (*name).trim().is_empty() {
                fs.set(FormState::Failed("Code and name are required.".to_string()));
                return;
            }
            let seq_val = (*seq).parse::<i16>().unwrap_or(1).max(1);
            let body = CreateOpsStopRequest {
                code:           (*code).clone(),
                name:           (*name).clone(),
                sequence_order: seq_val,
                latitude:       None,
                longitude:      None,
            };
            let fs2  = fs.clone();
            let load = load.clone();
            spawn_local(async move {
                fs2.set(FormState::Submitting);
                match ops_service::create_stop(route_id, &body).await {
                    Ok(_)  => { fs2.set(FormState::Hidden); load.emit(route_id); }
                    Err(e) => fs2.set(FormState::Failed(e)),
                }
            });
        })
    };

    let on_delete = {
        let dw   = del_working.clone();
        let sel  = sel_route_id.clone();
        let load = load_stops.clone();
        Callback::from(move |stop_id: Uuid| {
            let route_id = match *sel {
                Some(id) => id,
                None     => return,
            };
            let dw2  = dw.clone();
            let load = load.clone();
            spawn_local(async move {
                dw2.set(true);
                let _ = ops_service::delete_stop(route_id, stop_id).await;
                dw2.set(false);
                load.emit(route_id);
            });
        })
    };

    // ── Route selector ────────────────────────────────────────────────────────

    let route_options: Html = (*routes).iter().map(|r| {
        let id_str = r.id.to_string();
        let label  = format!("{} – {}", r.code, r.name);
        html! { <option value={id_str}>{ label }</option> }
    }).collect();

    // ── Stop table ────────────────────────────────────────────────────────────

    let body = match &*page_state {
        PageState::Init => html! {
            <div class="empty-state"><p>{ "Select a route to manage its stops." }</p></div>
        },
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! {
            <p class="error-state__message">{ e }</p>
        },
        PageState::Loaded(stops) if stops.is_empty() => html! {
            <div class="empty-state"><p>{ "No stops defined for this route." }</p></div>
        },
        PageState::Loaded(stops) => {
            let del = on_delete.clone();
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Seq" }</th>
                            <th>{ "Code" }</th>
                            <th>{ "Name" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for stops.iter().map(|s| {
                            let sid  = s.id;
                            let del2 = del.clone();
                            let is_del = *del_working;
                            html! {
                                <tr key={sid.to_string()}>
                                    <td>{ s.sequence_order }</td>
                                    <td class="mono">{ &s.code }</td>
                                    <td>{ &s.name }</td>
                                    <td class="action-cell">
                                        <button class="btn btn--small btn--danger"
                                                disabled={is_del}
                                                onclick={Callback::from(move |_| del2.emit(sid))}>
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

    // ── Add stop modal ────────────────────────────────────────────────────────

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
            html! {
                <div class="modal-overlay">
                    <div class="modal">
                        <h2 class="modal__title">{ "Add Stop" }</h2>
                        { err_msg }
                        <form onsubmit={on_create} class="modal__form">
                            <label class="form-field">
                                <span>{ "Code" }</span>
                                <input type="text" class="form-field__input"
                                       oninput={{
                                           let c = code_input.clone();
                                           Callback::from(move |e: InputEvent| {
                                               let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                               c.set(el.value());
                                           })
                                       }} />
                            </label>
                            <label class="form-field">
                                <span>{ "Name" }</span>
                                <input type="text" class="form-field__input"
                                       oninput={{
                                           let n = name_input.clone();
                                           Callback::from(move |e: InputEvent| {
                                               let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                               n.set(el.value());
                                           })
                                       }} />
                            </label>
                            <label class="form-field">
                                <span>{ "Sequence order" }</span>
                                <input type="number" min="1" class="form-field__input"
                                       value={(*seq_input).clone()}
                                       oninput={{
                                           let s = seq_input.clone();
                                           Callback::from(move |e: InputEvent| {
                                               let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                               s.set(el.value());
                                           })
                                       }} />
                            </label>
                            <div class="form-actions">
                                <button type="submit" class="btn btn--primary"
                                        disabled={is_sub}>
                                    { if is_sub { "Adding…" } else { "Add Stop" } }
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
                <h1 class="page__title">{ "Stop Management" }</h1>
                <div class="page__actions">
                    if sel_route_id.is_some() {
                        <button class="btn btn--primary"
                                onclick={Callback::from({
                                    let fs = form_state.clone();
                                    move |_| fs.set(FormState::Visible)
                                })}>
                            { "+ Add Stop" }
                        </button>
                    }
                </div>
            </header>
            <div class="page__body">
                <div class="filter-bar">
                    <label class="form-field form-field--inline">
                        <span>{ "Route" }</span>
                        <select class="form-field__input" onchange={on_route_select}>
                            <option value="">{ "— Select a route —" }</option>
                            { route_options }
                        </select>
                    </label>
                </div>
                { body }
            </div>
            { form_modal }
        </div>
    }
}
