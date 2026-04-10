/// Reporting: Scheduled report management page.
///
/// Lists scheduled reports, allows creating/deleting schedules and
/// triggering a manual run.  All mutations are reauth-gated.
///
/// Bug fix: create failure on 403 previously mapped to
/// `PendingAction::Delete(Uuid::nil())` which caused the reauth retry to
/// attempt to delete a nil-UUID row.  Now uses `PendingAction::Create` and
/// re-invokes the actual create callback on reauth success.
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    components::reauth_prompt::ReauthPrompt,
    services::reporting_service,
    types::reporting::{CreateScheduledReportRequest, ScheduledReport},
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<ScheduledReport>), Error(String) }

#[derive(Clone, PartialEq)]
enum ActionState {
    Idle,
    Working,
    ReauthRequired { pending: PendingAction },
    Done(String),
    Failed(String),
}

/// All actions that may require re-authentication.
///
/// `Create` stores no payload — inputs are re-read from component state on
/// retry, which is fine because the form is still mounted when reauth occurs.
#[derive(Clone, PartialEq)]
enum PendingAction {
    Create,
    Delete(Uuid),
    Trigger(Uuid),
}

#[function_component(SchedulesPage)]
pub fn schedules_page() -> Html {
    let page_state   = use_state(|| PageState::Loading);
    let action_state = use_state(|| ActionState::Idle);
    let name_input   = use_state(|| "Weekly Report".to_string());
    let cron_input   = use_state(|| "0 8 * * 1".to_string());

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match reporting_service::list_schedules().await {
                    Ok(items) => ps.set(PageState::Loaded(items)),
                    Err(e)    => ps.set(PageState::Error(e)),
                }
            });
        }
    };

    { let r = reload.clone(); use_effect_with((), move |_| { r(); || () }); }

    let handle_err = {
        let ast = action_state.clone();
        move |e: String, pending: PendingAction| {
            if e.contains("[403]") {
                ast.set(ActionState::ReauthRequired { pending });
            } else {
                ast.set(ActionState::Failed(e));
            }
        }
    };

    // ── Create — extracted so reauth retry can call it directly ──────────────

    let do_create = {
        let ast   = action_state.clone();
        let ni    = name_input.clone();
        let ci    = cron_input.clone();
        let rel   = reload.clone();
        let herr  = handle_err.clone();
        Callback::from(move |_: ()| {
            let body = CreateScheduledReportRequest {
                name:            (*ni).clone(),
                metric_ids:      vec![],
                schedule:        (*ci).clone(),
                date_range_days: Some(30),
                output_format:   Some("csv".to_string()),
            };
            if body.name.trim().is_empty() {
                ast.set(ActionState::Failed("Schedule name is required.".to_string()));
                return;
            }
            if body.schedule.trim().is_empty() {
                ast.set(ActionState::Failed("Cron expression is required.".to_string()));
                return;
            }
            let ast2  = ast.clone();
            let rel2  = rel.clone();
            let herr2 = herr.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match reporting_service::create_schedule(&body).await {
                    Ok(_)  => { ast2.set(ActionState::Done("Schedule created".into())); rel2(); }
                    Err(e) => herr2(e, PendingAction::Create),
                }
            });
        })
    };

    let create_schedule = {
        let dc = do_create.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            dc.emit(());
        })
    };

    // ── Delete ────────────────────────────────────────────────────────────────

    let delete_schedule = {
        let ast  = action_state.clone();
        let rel  = reload.clone();
        let herr = handle_err.clone();
        Callback::from(move |sid: Uuid| {
            let ast2  = ast.clone();
            let rel2  = rel.clone();
            let herr2 = herr.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match reporting_service::delete_schedule(sid).await {
                    Ok(_)  => { ast2.set(ActionState::Done("Schedule deleted".into())); rel2(); }
                    Err(e) => herr2(e, PendingAction::Delete(sid)),
                }
            });
        })
    };

    // ── Trigger run ───────────────────────────────────────────────────────────

    let trigger_run = {
        let ast  = action_state.clone();
        let herr = handle_err.clone();
        Callback::from(move |sid: Uuid| {
            let ast2  = ast.clone();
            let herr2 = herr.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match reporting_service::trigger_run(sid).await {
                    Ok(_)  => ast2.set(ActionState::Done("Run triggered".into())),
                    Err(e) => herr2(e, PendingAction::Trigger(sid)),
                }
            });
        })
    };

    // ── Table content ─────────────────────────────────────────────────────────

    let content = match &*page_state {
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        PageState::Loaded(items) if items.is_empty() => html! {
            <div class="empty-state"><p>{ "No scheduled reports." }</p></div>
        },
        PageState::Loaded(items) => {
            let del  = delete_schedule.clone();
            let trig = trigger_run.clone();
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Name" }</th>
                            <th>{ "Schedule" }</th>
                            <th>{ "Format" }</th>
                            <th>{ "Next Run" }</th>
                            <th>{ "Active" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for items.iter().map(|s| {
                            let sid   = s.id;
                            let del2  = del.clone();
                            let trig2 = trig.clone();
                            html! {
                                <tr key={sid.to_string()}>
                                    <td>{ &s.name }</td>
                                    <td class="mono">{ &s.schedule }</td>
                                    <td>{ &s.output_format }</td>
                                    <td>
                                        { s.next_run_at
                                            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                                            .unwrap_or_else(|| "—".to_string()) }
                                    </td>
                                    <td>{ if s.is_active { "Yes" } else { "No" } }</td>
                                    <td class="action-cell">
                                        <button class="btn btn--small btn--secondary"
                                                onclick={Callback::from(move |_| trig2.emit(sid))}>
                                            { "Run Now" }
                                        </button>
                                        <button class="btn btn--small btn--danger"
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

    // ── Feedback / reauth ─────────────────────────────────────────────────────

    let feedback = match &*action_state {
        ActionState::Working   => html! { <div class="action-feedback action-feedback--working">{ "Working…" }</div> },
        ActionState::Done(msg) => html! { <div class="action-feedback action-feedback--success">{ msg }</div> },
        ActionState::Failed(e) => html! { <div class="action-feedback action-feedback--error">{ e }</div> },
        _ => html! {},
    };

    let reauth_overlay = if let ActionState::ReauthRequired { pending } = &*action_state {
        let pending    = pending.clone();
        let ast        = action_state.clone();
        let del_cb     = delete_schedule.clone();
        let trig_cb    = trigger_run.clone();
        let create_cb  = do_create.clone();
        html! {
            <ReauthPrompt
                on_success={Callback::from(move |_| {
                    match &pending {
                        // Re-invoke the correct action after the user re-authenticates.
                        PendingAction::Create      => create_cb.emit(()),
                        PendingAction::Delete(id)  => del_cb.emit(*id),
                        PendingAction::Trigger(id) => trig_cb.emit(*id),
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
                <h1 class="page__title">{ "Report Schedules" }</h1>
            </header>
            <div class="page__body">
                <form onsubmit={create_schedule} class="inline-form">
                    <label class="form-field form-field--inline">
                        <span>{ "Name" }</span>
                        <input type="text"
                               value={(*name_input).clone()}
                               oninput={{
                                   let n = name_input.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       n.set(el.value());
                                   })
                               }}
                               class="form-field__input" />
                    </label>
                    <label class="form-field form-field--inline">
                        <span>{ "Cron" }</span>
                        <input type="text"
                               value={(*cron_input).clone()}
                               oninput={{
                                   let c = cron_input.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       c.set(el.value());
                                   })
                               }}
                               class="form-field__input" />
                    </label>
                    <button type="submit" class="btn btn--primary"
                            disabled={matches!(&*action_state, ActionState::Working)}>
                        { if matches!(&*action_state, ActionState::Working) { "Working…" } else { "Schedule" } }
                    </button>
                </form>
                { feedback }
                { content }
            </div>
            { reauth_overlay }
        </div>
    }
}
