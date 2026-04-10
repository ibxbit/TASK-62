/// Finance: Reconciliation runs page.
///
/// Displays reconciliation run history and lets the analyst trigger a new run.
/// Triggering a run is reauth-gated (server returns 403 if session is stale).
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    components::reauth_prompt::ReauthPrompt,
    services::finance_service,
    types::finance::{ReconciliationRun, StartRunRequest},
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<ReconciliationRun>), Error(String) }

#[derive(Clone, PartialEq)]
enum ActionState { Idle, Working, ReauthRequired { import_id: Uuid }, Done(String), Failed(String) }

#[function_component(ReconciliationPage)]
pub fn reconciliation_page() -> Html {
    let page_state   = use_state(|| PageState::Loading);
    let action_state = use_state(|| ActionState::Idle);
    let import_input = use_state(String::new);

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match finance_service::list_runs().await {
                    Ok(runs) => ps.set(PageState::Loaded(runs)),
                    Err(e)   => ps.set(PageState::Error(e)),
                }
            });
        }
    };

    { let r = reload.clone(); use_effect_with((), move |_| { r(); || () }); }

    let trigger_run = {
        let ast    = action_state.clone();
        let reload = reload.clone();
        Callback::from(move |import_id: Uuid| {
            let ast2   = ast.clone();
            let reload = reload.clone();
            let body = StartRunRequest {
                statement_import_id: import_id,
                run_date: chrono::Utc::now().date_naive().to_string(),
            };
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match finance_service::start_run(&body).await {
                    Ok(_) => {
                        ast2.set(ActionState::Done("Reconciliation run started".to_string()));
                        reload();
                    }
                    Err(e) => {
                        if e.contains("[403]") {
                            ast2.set(ActionState::ReauthRequired { import_id });
                        } else {
                            ast2.set(ActionState::Failed(e));
                        }
                    }
                }
            });
        })
    };

    let on_trigger = {
        let imp     = import_input.clone();
        let trig_cb = trigger_run.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            if let Ok(uid) = (*imp).parse::<Uuid>() {
                trig_cb.emit(uid);
            }
        })
    };

    let content = match &*page_state {
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        PageState::Loaded(runs) if runs.is_empty() => html! {
            <div class="empty-state"><p>{ "No reconciliation runs yet." }</p></div>
        },
        PageState::Loaded(runs) => html! {
            <table class="data-table">
                <thead>
                    <tr>
                        <th>{ "Run Date" }</th>
                        <th>{ "Status" }</th>
                        <th>{ "Discrepancies" }</th>
                        <th>{ "Net" }</th>
                        <th>{ "Started" }</th>
                        <th>{ "Completed" }</th>
                    </tr>
                </thead>
                <tbody>
                    { for runs.iter().map(|r| html! {
                        <tr key={r.id.to_string()}>
                            <td>{ r.run_date.to_string() }</td>
                            <td>
                                <span class={format!("badge badge--{}", r.status)}>{ &r.status }</span>
                            </td>
                            <td>{ r.discrepancy_count }</td>
                            <td>{ format!("{:.2}", r.net_discrepancy()) }</td>
                            <td>{ r.started_at.format("%Y-%m-%d %H:%M UTC").to_string() }</td>
                            <td>
                                { r.completed_at
                                    .map(|t| t.format("%H:%M UTC").to_string())
                                    .unwrap_or_else(|| "—".to_string()) }
                            </td>
                        </tr>
                    }) }
                </tbody>
            </table>
        },
    };

    let feedback = match &*action_state {
        ActionState::Working   => html! { <div class="action-feedback action-feedback--working">{ "Starting run…" }</div> },
        ActionState::Done(msg) => html! { <div class="action-feedback action-feedback--success">{ msg }</div> },
        ActionState::Failed(e) => html! { <div class="action-feedback action-feedback--error">{ e }</div> },
        _ => html! {},
    };

    let reauth_overlay = if let ActionState::ReauthRequired { import_id } = &*action_state {
        let iid  = *import_id;
        let ast  = action_state.clone();
        let trig = trigger_run.clone();
        html! {
            <ReauthPrompt
                on_success={Callback::from(move |_| trig.emit(iid))}
                on_cancel={Callback::from(move |_| ast.set(ActionState::Idle))}
            />
        }
    } else { html! {} };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Reconciliation" }</h1>
            </header>
            <div class="page__body">
                <form onsubmit={on_trigger} class="inline-form">
                    <label class="form-field form-field--inline">
                        <span>{ "Statement Import ID" }</span>
                        <input type="text" placeholder="uuid"
                               oninput={{
                                   let i = import_input.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       i.set(el.value());
                                   })
                               }}
                               class="form-field__input" />
                    </label>
                    <button type="submit" class="btn btn--primary">{ "Start Run" }</button>
                </form>
                { feedback }
                { content }
            </div>
            { reauth_overlay }
        </div>
    }
}
