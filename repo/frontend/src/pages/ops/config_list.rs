/// Ops config version list page.
///
/// Flow:
///   1. On mount: load all config templates from GET /ops/configs.
///   2. User selects a template → versions for that template are fetched.
///   3. Admin can Publish / Unpublish / Schedule (inline datetime form) any version.
///   4. Admin can create a new draft via the "+ New Draft" button, optionally
///      basing it on an existing version for incremental changes.
///
/// All mutating actions are reauth-gated: a 403 response shows ReauthPrompt
/// and retries the action automatically on success.
use chrono::{DateTime, NaiveDateTime, Utc};
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    components::reauth_prompt::ReauthPrompt,
    services::ops_service,
    types::ops::{ConfigTemplate, ConfigVersion, CreateVersionRequest, ScheduleVersionRequest},
};

pub const EMPTY_STATE_DRAFT_HINT: &str = "Use \"+ New Draft\" above to create the first version.";

// ── State enums ───────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum TemplateState { Loading, Loaded(Vec<ConfigTemplate>), Error(String) }

#[derive(Clone, PartialEq)]
enum PageState { Idle, Loading, Loaded(Vec<ConfigVersion>), Error(String) }

/// `PendingAction` carries all context needed for a reauth retry so the retry
/// callback doesn't need to close over additional state.
#[derive(Clone, PartialEq, Debug)]
enum PendingAction {
    Publish   { vid: Uuid, tid: Uuid },
    Unpublish { vid: Uuid, tid: Uuid },
    Schedule  { vid: Uuid, tid: Uuid },
    /// CreateDraft carries no version_id — inputs re-read from component state
    /// on retry (form still mounted during the reauth prompt).
    CreateDraft { tid: Uuid },
}

#[derive(Clone, PartialEq)]
enum ActionState {
    Idle,
    Working,
    ReauthRequired { pending: PendingAction },
    Done(String),
    Failed(String),
}

#[derive(Clone, PartialEq)]
enum FormState { Hidden, Visible, Submitting, Failed(String) }

// ── Helper: parse a datetime-local string ("YYYY-MM-DDTHH:MM") ───────────────

fn parse_schedule_dt(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .ok()
        .map(|ndt| DateTime::from_naive_utc_and_offset(ndt, Utc))
}

// ── Component ─────────────────────────────────────────────────────────────────

#[function_component(ConfigListPage)]
pub fn config_list_page() -> Html {
    // ── Template layer ────────────────────────────────────────────────────────
    let template_state = use_state(|| TemplateState::Loading);
    let selected_tmpl  = use_state::<Option<Uuid>, _>(|| None);

    // ── Version layer ─────────────────────────────────────────────────────────
    let page_state      = use_state(|| PageState::Idle);
    let action_state    = use_state(|| ActionState::Idle);
    let schedule_open   = use_state::<Option<Uuid>, _>(|| None);
    let schedule_dt_inp = use_state(String::new);

    // ── Create-draft form ─────────────────────────────────────────────────────
    let draft_form     = use_state(|| FormState::Hidden);
    let draft_based_on = use_state::<Option<Uuid>, _>(|| None);

    // ── Load templates on mount ───────────────────────────────────────────────
    {
        let ts = template_state.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match ops_service::list_templates().await {
                    Ok(tmpls) => ts.set(TemplateState::Loaded(tmpls)),
                    Err(e)    => ts.set(TemplateState::Error(e)),
                }
            });
            || ()
        });
    }

    // ── Load versions for the selected template ───────────────────────────────

    let reload_versions = {
        let ps  = page_state.clone();
        let sel = selected_tmpl.clone();
        move |tid: Uuid| {
            sel.set(Some(tid));
            let ps2 = ps.clone();
            ps2.set(PageState::Loading);
            spawn_local(async move {
                match ops_service::list_versions(tid).await {
                    Ok(data) => {
                        let versions: Vec<ConfigVersion> =
                            data.get("data")
                                .and_then(|v| serde_json::from_value(v.clone()).ok())
                                .unwrap_or_default();
                        ps2.set(PageState::Loaded(versions));
                    }
                    Err(e) => ps2.set(PageState::Error(e)),
                }
            });
        }
    };

    // ── Shared 403 handler ────────────────────────────────────────────────────

    let handle_403 = {
        let ast = action_state.clone();
        move |e: String, pending: PendingAction| {
            if e.contains("[403]") {
                ast.set(ActionState::ReauthRequired { pending });
            } else {
                ast.set(ActionState::Failed(e));
            }
        }
    };

    // ── Publish ───────────────────────────────────────────────────────────────

    let do_publish = {
        let ast  = action_state.clone();
        let herr = handle_403.clone();
        Callback::from(move |(vid, tid): (Uuid, Uuid)| {
            let ast2  = ast.clone();
            let herr2 = herr.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match ops_service::publish_version(tid, vid).await {
                    Ok(_)  => ast2.set(ActionState::Done("Version published.".into())),
                    Err(e) => herr2(e, PendingAction::Publish { vid, tid }),
                }
            });
        })
    };

    // ── Unpublish ─────────────────────────────────────────────────────────────

    let do_unpublish = {
        let ast  = action_state.clone();
        let herr = handle_403.clone();
        Callback::from(move |(vid, tid): (Uuid, Uuid)| {
            let ast2  = ast.clone();
            let herr2 = herr.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match ops_service::unpublish_version(tid, vid).await {
                    Ok(_)  => ast2.set(ActionState::Done("Version unpublished.".into())),
                    Err(e) => herr2(e, PendingAction::Unpublish { vid, tid }),
                }
            });
        })
    };

    // ── Schedule ──────────────────────────────────────────────────────────────

    let do_schedule = {
        let ast     = action_state.clone();
        let herr    = handle_403.clone();
        let sdt     = schedule_dt_inp.clone();
        let so      = schedule_open.clone();
        Callback::from(move |(vid, tid): (Uuid, Uuid)| {
            let dt_str = (*sdt).clone();
            let effective_from = match parse_schedule_dt(&dt_str) {
                Some(dt) => dt,
                None => {
                    ast.set(ActionState::Failed(
                        "Invalid date — use the date/time picker.".to_string(),
                    ));
                    return;
                }
            };
            let body = ScheduleVersionRequest { effective_from, effective_to: None };
            let ast2  = ast.clone();
            let herr2 = herr.clone();
            let so2   = so.clone();
            spawn_local(async move {
                so2.set(None);
                ast2.set(ActionState::Working);
                match ops_service::schedule_version(tid, vid, &body).await {
                    Ok(_)  => ast2.set(ActionState::Done("Version scheduled.".into())),
                    Err(e) => herr2(e, PendingAction::Schedule { vid, tid }),
                }
            });
        })
    };

    // ── Create draft — extracted so reauth retry can call it directly ─────────

    let do_create_draft = {
        let ast   = action_state.clone();
        let herr  = handle_403.clone();
        let df    = draft_form.clone();
        let based = draft_based_on.clone();
        let sel   = selected_tmpl.clone();
        let ps    = page_state.clone();
        Callback::from(move |_: ()| {
            let tid = match *sel {
                Some(t) => t,
                None    => {
                    df.set(FormState::Failed("Select a template first.".to_string()));
                    return;
                }
            };
            let body = CreateVersionRequest {
                payload:          serde_json::json!({}),
                based_on_version: *based,
            };
            let ast2  = ast.clone();
            let herr2 = herr.clone();
            let df2   = df.clone();
            let ps2   = ps.clone();
            let sel2  = sel.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                df2.set(FormState::Submitting);
                match ops_service::create_version(tid, &body).await {
                    Ok(_) => {
                        ast2.set(ActionState::Done("Draft created.".into()));
                        df2.set(FormState::Hidden);
                        // Reload versions to show the new draft
                        let ps3 = ps2.clone();
                        ps3.set(PageState::Loading);
                        if let Some(tid2) = *sel2 {
                            spawn_local(async move {
                                match ops_service::list_versions(tid2).await {
                                    Ok(data) => {
                                        let versions: Vec<ConfigVersion> =
                                            data.get("data")
                                                .and_then(|v| serde_json::from_value(v.clone()).ok())
                                                .unwrap_or_default();
                                        ps3.set(PageState::Loaded(versions));
                                    }
                                    Err(e) => ps3.set(PageState::Error(e)),
                                }
                            });
                        }
                    }
                    Err(e) => {
                        df2.set(FormState::Visible);
                        herr2(e, PendingAction::CreateDraft { tid });
                    }
                }
            });
        })
    };

    let on_draft_submit = {
        let dc = do_create_draft.clone();
        Callback::from(move |e: SubmitEvent| { e.prevent_default(); dc.emit(()); })
    };

    // ── Template selector ─────────────────────────────────────────────────────

    let template_selector = match &*template_state {
        TemplateState::Loading => html! {
            <div class="loading-state inline-loading">
                <span>{ "Loading templates…" }</span>
            </div>
        },
        TemplateState::Error(e) => html! {
            <p class="error-state__message">{ format!("Could not load templates: {}", e) }</p>
        },
        TemplateState::Loaded(tmpls) if tmpls.is_empty() => html! {
            <p class="empty-state__message">{ "No config templates found." }</p>
        },
        TemplateState::Loaded(tmpls) => {
            let rel = reload_versions.clone();
            html! {
                <div class="template-selector">
                    <label class="form-field form-field--inline">
                        <span>{ "Template" }</span>
                        <select class="form-field__input"
                                onchange={{
                                    let tmpls2 = tmpls.clone();
                                    Callback::from(move |e: Event| {
                                        let el: web_sys::HtmlSelectElement = e.target_unchecked_into();
                                        let val = el.value();
                                        if let Ok(tid) = val.parse::<Uuid>() {
                                            rel(tid);
                                            // also populate draft_based_on dropdown if needed
                                        }
                                        let _ = tmpls2.len(); // keep borrow alive
                                    })
                                }}>
                            <option value="">{ "— select a template —" }</option>
                            { for tmpls.iter().map(|t| {
                                html! {
                                    <option value={t.id.to_string()}>
                                        { &t.key }
                                    </option>
                                }
                            }) }
                        </select>
                    </label>
                </div>
            }
        }
    };

    // ── Version table ─────────────────────────────────────────────────────────

    let content = match &*page_state {
        PageState::Idle => html! {
            <div class="empty-state">
                <p>{ "Select a template above to view its versions." }</p>
            </div>
        },
        PageState::Loading => html! {
            <div class="loading-state">
                <div class="spinner" />
                <p>{ "Loading versions…" }</p>
            </div>
        },
        PageState::Error(e) => html! {
            <div class="error-state">
                <p class="error-state__message">{ format!("Error: {}", e) }</p>
            </div>
        },
        PageState::Loaded(versions) if versions.is_empty() => html! {
            <div class="empty-state">
                <p class="empty-state__message">{ "No versions yet for this template." }</p>
                <p>{ EMPTY_STATE_DRAFT_HINT }</p>
            </div>
        },
        PageState::Loaded(versions) => {
            let tid       = (*selected_tmpl).unwrap_or(Uuid::nil());
            let pub_cb    = do_publish.clone();
            let unpub_cb  = do_unpublish.clone();
            let sch_cb    = do_schedule.clone();
            let so        = schedule_open.clone();
            let sdt       = schedule_dt_inp.clone();
            let is_work   = matches!(&*action_state, ActionState::Working);
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Version" }</th>
                            <th>{ "Status" }</th>
                            <th>{ "Published At" }</th>
                            <th>{ "Effective From" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for versions.iter().map(|v| {
                            let vid       = v.id;
                            let is_draft  = v.is_draft();
                            let is_pub    = v.is_published();
                            let pub_cb2   = pub_cb.clone();
                            let upub_cb2  = unpub_cb.clone();
                            let sch_cb2   = sch_cb.clone();
                            let so2       = so.clone();
                            let so3       = so.clone();
                            let sdt2      = sdt.clone();
                            let form_open = *so == Some(vid);
                            html! {
                                <>
                                <tr key={vid.to_string()}>
                                    <td>{ format!("v{}", v.version_number) }</td>
                                    <td>
                                        <span class={format!("badge badge--{}", v.status)}>
                                            { v.status_label() }
                                        </span>
                                    </td>
                                    <td>
                                        { v.published_at
                                            .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
                                            .unwrap_or_else(|| "—".to_string()) }
                                    </td>
                                    <td>
                                        { v.effective_from
                                            .map(|t| t.format("%Y-%m-%d").to_string())
                                            .unwrap_or_else(|| "—".to_string()) }
                                    </td>
                                    <td class="action-cell">
                                        if is_draft {
                                            <button class="btn btn--small btn--primary"
                                                    disabled={is_work}
                                                    onclick={Callback::from(move |_| pub_cb2.emit((vid, tid)))}>
                                                { "Publish" }
                                            </button>
                                            { " " }
                                            <button class="btn btn--small btn--secondary"
                                                    disabled={is_work}
                                                    onclick={Callback::from(move |_| so2.set(Some(vid)))}>
                                                { "Schedule" }
                                            </button>
                                        }
                                        if is_pub {
                                            <button class="btn btn--small btn--danger"
                                                    disabled={is_work}
                                                    onclick={Callback::from(move |_| upub_cb2.emit((vid, tid)))}>
                                                { "Unpublish" }
                                            </button>
                                        }
                                    </td>
                                </tr>
                                if form_open {
                                    <tr class="schedule-form-row" key={format!("sch-{}", vid)}>
                                        <td colspan="5">
                                            <div class="schedule-form">
                                                <label class="form-field form-field--inline">
                                                    <span>{ "Effective from (UTC)" }</span>
                                                    <input type="datetime-local"
                                                           class="form-field__input"
                                                           oninput={{
                                                               let sdt3 = sdt2.clone();
                                                               Callback::from(move |e: InputEvent| {
                                                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                                                   sdt3.set(el.value());
                                                               })
                                                           }} />
                                                </label>
                                                <div class="form-actions">
                                                    <button class="btn btn--small btn--primary"
                                                            onclick={Callback::from(move |_| sch_cb2.emit((vid, tid)))}>
                                                        { "Confirm" }
                                                    </button>
                                                    <button class="btn btn--small btn--secondary"
                                                            onclick={Callback::from({
                                                                move |_| so3.set(None)
                                                            })}>
                                                        { "Cancel" }
                                                    </button>
                                                </div>
                                            </div>
                                        </td>
                                    </tr>
                                }
                                </>
                            }
                        }) }
                    </tbody>
                </table>
            }
        }
    };

    // ── Create draft modal ────────────────────────────────────────────────────

    let show_draft_form  = !matches!(&*draft_form, FormState::Hidden);
    let draft_is_sub     = matches!(&*draft_form, FormState::Submitting);
    let draft_err        = if let FormState::Failed(e) = &*draft_form {
        html! { <p class="form-error">{ e }</p> }
    } else { html! {} };

    let available_versions = if let PageState::Loaded(ref vs) = *page_state {
        vs.clone()
    } else {
        vec![]
    };

    let draft_modal = if show_draft_form {
        let cancel = {
            let df = draft_form.clone();
            Callback::from(move |_: MouseEvent| df.set(FormState::Hidden))
        };
        html! {
            <div class="modal-overlay">
                <div class="modal">
                    <h2 class="modal__title">{ "Create New Draft" }</h2>
                    { draft_err }
                    <form onsubmit={on_draft_submit} class="modal__form">
                        <label class="form-field">
                            <span>{ "Base on existing version (optional)" }</span>
                            <select class="form-field__input"
                                    onchange={{
                                        let bo = draft_based_on.clone();
                                        Callback::from(move |e: Event| {
                                            let el: web_sys::HtmlSelectElement = e.target_unchecked_into();
                                            let val = el.value();
                                            bo.set(val.parse::<Uuid>().ok());
                                        })
                                    }}>
                                <option value="">{ "— start from scratch —" }</option>
                                { for available_versions.iter().map(|v| {
                                    html! {
                                        <option value={v.id.to_string()}>
                                            { format!("v{} ({})", v.version_number, v.status_label()) }
                                        </option>
                                    }
                                }) }
                            </select>
                        </label>
                        <p class="form-hint">
                            { "A new draft version will be created with an empty payload \
                               (or copied from the selected base version by the server)." }
                        </p>
                        <div class="form-actions">
                            <button type="submit" class="btn btn--primary"
                                    disabled={draft_is_sub}>
                                { if draft_is_sub { "Creating…" } else { "Create Draft" } }
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
    } else {
        html! {}
    };

    // ── Action feedback ───────────────────────────────────────────────────────

    let feedback = match &*action_state {
        ActionState::Working   => html! {
            <div class="action-feedback action-feedback--working">{ "Working…" }</div>
        },
        ActionState::Done(msg) => html! {
            <div class="action-feedback action-feedback--success">{ msg }</div>
        },
        ActionState::Failed(e) => html! {
            <div class="action-feedback action-feedback--error">{ e }</div>
        },
        _ => html! {}
    };

    // ── Reauth overlay ────────────────────────────────────────────────────────

    let reauth_overlay = if let ActionState::ReauthRequired { pending } = &*action_state {
        let pending      = pending.clone();
        let ast          = action_state.clone();
        let pub_re       = do_publish.clone();
        let unpub_re     = do_unpublish.clone();
        let sch_re       = do_schedule.clone();
        let create_re    = do_create_draft.clone();
        html! {
            <ReauthPrompt
                on_success={Callback::from(move |_| {
                    match &pending {
                        PendingAction::Publish   { vid, tid } => pub_re.emit((*vid, *tid)),
                        PendingAction::Unpublish { vid, tid } => unpub_re.emit((*vid, *tid)),
                        PendingAction::Schedule  { vid, tid } => sch_re.emit((*vid, *tid)),
                        PendingAction::CreateDraft { .. }     => create_re.emit(()),
                    }
                })}
                on_cancel={Callback::from(move |_| ast.set(ActionState::Idle))}
            />
        }
    } else {
        html! {}
    };

    let can_create = matches!(*selected_tmpl, Some(_));

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Config Versions" }</h1>
                <div class="page__actions">
                    <button class="btn btn--primary"
                            disabled={!can_create}
                            title={if can_create { "Create new draft for selected template" } else { "Select a template first" }}
                            onclick={Callback::from({
                                let df = draft_form.clone();
                                move |_| df.set(FormState::Visible)
                            })}>
                        { "+ New Draft" }
                    </button>
                </div>
            </header>

            { feedback }

            <div class="page__body">
                { template_selector }
                { content }
            </div>

            { draft_modal }
            { reauth_overlay }
        </div>
    }
}
