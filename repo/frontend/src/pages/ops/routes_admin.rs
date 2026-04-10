/// Operations admin: Route management page.
///
/// Lists all routes and allows admins to create new routes and delete
/// existing ones.  Routes cannot be edited while active (the backend
/// enforces this); the admin must unpublish first.
///
/// States:
///   PageState:  Loading | Loaded(routes) | Error(msg)
///   FormState:  Hidden | Visible | Submitting | Failed(msg)
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::ops_service,
    types::ops::{CreateOpsRouteRequest, OpsRoute},
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<OpsRoute>), Error(String) }

#[derive(Clone, PartialEq)]
enum FormState { Hidden, Visible, Submitting, Failed(String) }

#[derive(Clone, PartialEq)]
enum DeleteState { Idle, Working(Uuid), Failed(String) }

#[function_component(RoutesAdminPage)]
pub fn routes_admin_page() -> Html {
    let page_state   = use_state(|| PageState::Loading);
    let form_state   = use_state(|| FormState::Hidden);
    let delete_state = use_state(|| DeleteState::Idle);

    // Form fields
    let code_input = use_state(String::new);
    let name_input = use_state(String::new);
    let desc_input = use_state(String::new);

    // ── Reload list ───────────────────────────────────────────────────────────

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match ops_service::list_routes_admin().await {
                    Ok(page) => ps.set(PageState::Loaded(page.data)),
                    Err(e)   => ps.set(PageState::Error(e)),
                }
            });
        }
    };

    { let r = reload.clone(); use_effect_with((), move |_| { r(); || () }); }

    // ── Create route ──────────────────────────────────────────────────────────

    let on_create = {
        let fs     = form_state.clone();
        let code   = code_input.clone();
        let name   = name_input.clone();
        let desc   = desc_input.clone();
        let reload = reload.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let code_val = (*code).trim().to_string();
            let name_val = (*name).trim().to_string();

            if code_val.is_empty() || name_val.is_empty() {
                fs.set(FormState::Failed("Route code and name are required.".to_string()));
                return;
            }

            let body = CreateOpsRouteRequest {
                code:           code_val,
                name:           name_val,
                description:    {
                    let d = (*desc).trim().to_string();
                    if d.is_empty() { None } else { Some(d) }
                },
                effective_from: None,
            };

            let fs2    = fs.clone();
            let code2  = code.clone();
            let name2  = name.clone();
            let desc2  = desc.clone();
            let reload = reload.clone();
            spawn_local(async move {
                fs2.set(FormState::Submitting);
                match ops_service::create_route_admin(&body).await {
                    Ok(_) => {
                        fs2.set(FormState::Hidden);
                        code2.set(String::new());
                        name2.set(String::new());
                        desc2.set(String::new());
                        reload();
                    }
                    Err(e) => fs2.set(FormState::Failed(e)),
                }
            });
        })
    };

    // ── Delete route ──────────────────────────────────────────────────────────

    let make_delete = {
        let ds     = delete_state.clone();
        let reload = reload.clone();
        move |route_id: Uuid| {
            let ds     = ds.clone();
            let reload = reload.clone();
            Callback::from(move |_: MouseEvent| {
                let ds2    = ds.clone();
                let reload = reload.clone();
                spawn_local(async move {
                    ds2.set(DeleteState::Working(route_id));
                    match ops_service::delete_route_admin(route_id).await {
                        Ok(_)  => { ds2.set(DeleteState::Idle); reload(); }
                        Err(e) => ds2.set(DeleteState::Failed(e)),
                    }
                });
            })
        }
    };

    // ── Render ────────────────────────────────────────────────────────────────

    let is_submitting = matches!(*form_state, FormState::Submitting);

    let form = match &*form_state {
        FormState::Hidden => html! {
            <button class="btn btn--primary"
                    onclick={{
                        let fs = form_state.clone();
                        Callback::from(move |_| fs.set(FormState::Visible))
                    }}>
                { "New Route" }
            </button>
        },
        FormState::Visible | FormState::Submitting | FormState::Failed(_) => html! {
            <form onsubmit={on_create} class="card card--form">
                <h3 class="card__title">{ "Create Route" }</h3>

                if let FormState::Failed(e) = &*form_state {
                    <p class="form-field__error">{ e }</p>
                }

                <label class="form-field">
                    <span>{ "Code" }</span>
                    <input type="text" placeholder="e.g. R001"
                           disabled={is_submitting}
                           class="form-field__input"
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
                    <input type="text" placeholder="Route display name"
                           disabled={is_submitting}
                           class="form-field__input"
                           oninput={{
                               let n = name_input.clone();
                               Callback::from(move |e: InputEvent| {
                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                   n.set(el.value());
                               })
                           }} />
                </label>
                <label class="form-field">
                    <span>{ "Description" }</span>
                    <input type="text" placeholder="Optional"
                           disabled={is_submitting}
                           class="form-field__input"
                           oninput={{
                               let d = desc_input.clone();
                               Callback::from(move |e: InputEvent| {
                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                   d.set(el.value());
                               })
                           }} />
                </label>
                <div class="form-actions">
                    <button type="submit"
                            class="btn btn--primary"
                            disabled={is_submitting}>
                        { if is_submitting { "Creating…" } else { "Create" } }
                    </button>
                    <button type="button"
                            class="btn btn--secondary"
                            disabled={is_submitting}
                            onclick={{
                                let fs = form_state.clone();
                                Callback::from(move |_| fs.set(FormState::Hidden))
                            }}>
                        { "Cancel" }
                    </button>
                </div>
            </form>
        },
    };

    let delete_feedback = match &*delete_state {
        DeleteState::Failed(e) => html! {
            <div class="action-feedback action-feedback--error">{ e }</div>
        },
        _ => html! {},
    };

    let content = match &*page_state {
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        PageState::Loaded(routes) if routes.is_empty() => html! {
            <div class="empty-state">
                <p>{ "No routes defined yet. Create the first one above." }</p>
            </div>
        },
        PageState::Loaded(routes) => {
            let deleting_id = match *delete_state {
                DeleteState::Working(id) => Some(id),
                _ => None,
            };
            let mk = make_delete.clone();
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Code" }</th>
                            <th>{ "Name" }</th>
                            <th>{ "Status" }</th>
                            <th>{ "Version" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for routes.iter().map(|r| {
                            let is_deleting = deleting_id == Some(r.id);
                            let del_cb = mk(r.id);
                            html! {
                                <tr key={r.id.to_string()}>
                                    <td>{ &r.code }</td>
                                    <td>{ &r.name }</td>
                                    <td>
                                        <span class={format!("badge badge--{}", r.status)}>
                                            { &r.status }
                                        </span>
                                    </td>
                                    <td>{ r.version }</td>
                                    <td>
                                        <button class="btn btn--danger btn--small"
                                                disabled={is_deleting}
                                                onclick={del_cb}>
                                            { if is_deleting { "Deleting…" } else { "Delete" } }
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

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Route Management" }</h1>
            </header>
            <div class="page__body">
                { form }
                { delete_feedback }
                { content }
            </div>
        </div>
    }
}
