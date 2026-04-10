/// Rollout manager page.
///
/// Two sections:
///
/// ## Create Rollout
/// Allows an admin to create a new staged rollout plan for a published config
/// version.  Each stage specifies a target percentage and a list of depot UUIDs
/// to include.  Validation:
///   - At least one stage required.
///   - Each stage: target_percentage > 0 and ≤ 100.
///   - Each stage: at least one valid depot UUID.
///   - template_id and version_id must parse as valid UUIDs.
///
/// On success the created plan is automatically loaded into the "Manage" section.
///
/// ## Manage / Activate Stages
/// Allows loading an existing rollout plan by template ID + plan ID and
/// activating individual stages sequentially.  Stage activation is reauth-
/// gated; a 403 response shows ReauthPrompt and retries automatically.
use chrono::{DateTime, NaiveDateTime, Utc};
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    components::reauth_prompt::ReauthPrompt,
    services::ops_service,
    types::ops::{CreateRolloutRequest, RolloutPlan, RolloutStageSpec},
};

// ── State enums ───────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum PageState { Empty, Loading, Loaded(RolloutPlan), Error(String) }

#[derive(Clone, PartialEq)]
enum ActionState {
    Idle,
    Working,
    ReauthRequired { stage_id: Uuid, plan_id: Uuid, template_id: Uuid },
    Done(String),
    Failed(String),
}

#[derive(Clone, PartialEq)]
enum CreateState { Idle, Working, Failed(String) }

// ── Stage input (one row in the stage builder) ────────────────────────────────

/// Editable representation of one rollout stage before submission.
#[derive(Clone, PartialEq, Default)]
struct StageInput {
    /// Percentage string as typed, e.g. "33".
    target_pct:   String,
    /// Comma-separated depot UUIDs, e.g. "uuid1, uuid2, uuid3".
    depot_ids_raw: String,
    /// Optional datetime-local string "YYYY-MM-DDTHH:MM" for scheduled activation.
    scheduled_at:  String,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_schedule_dt(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .ok()
        .map(|ndt| DateTime::from_naive_utc_and_offset(ndt, Utc))
}

/// Parse a comma-separated string of UUIDs; returns Err on the first invalid token.
pub fn parse_depot_ids(raw: &str) -> Result<Vec<Uuid>, String> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<Uuid>().map_err(|_| format!("Invalid depot UUID: \"{}\"", s)))
        .collect()
}

pub fn precheck_activate_ids(template_input: &str, plan_input: &str) -> Result<(Uuid, Uuid), String> {
    let tid = template_input
        .parse::<Uuid>()
        .map_err(|_| "Template ID is missing — load a plan before activating.".to_string())?;
    let pid = plan_input
        .parse::<Uuid>()
        .map_err(|_| "Plan ID is missing — load a plan before activating.".to_string())?;
    Ok((tid, pid))
}

// ── Component ─────────────────────────────────────────────────────────────────

#[function_component(RolloutManagerPage)]
pub fn rollout_manager_page() -> Html {

    // ── Create rollout form state ─────────────────────────────────────────────
    let create_tmpl_id  = use_state(String::new);
    let create_ver_id   = use_state(String::new);
    let create_notes    = use_state(String::new);
    let create_stages   = use_state(|| vec![StageInput {
        target_pct:    "100".to_string(),
        depot_ids_raw: String::new(),
        scheduled_at:  String::new(),
    }]);
    let create_state    = use_state(|| CreateState::Idle);
    let show_create     = use_state(|| false);

    // ── Load/manage form state ────────────────────────────────────────────────
    let template_input = use_state(String::new);
    let plan_input     = use_state(String::new);
    let page_state     = use_state(|| PageState::Empty);
    let action_state   = use_state(|| ActionState::Idle);

    // ── Create rollout submit ─────────────────────────────────────────────────

    let on_create_rollout = {
        let ctid     = create_tmpl_id.clone();
        let cvid     = create_ver_id.clone();
        let cnotes   = create_notes.clone();
        let cstages  = create_stages.clone();
        let cstate   = create_state.clone();
        let ps       = page_state.clone();
        let plan_inp = plan_input.clone();
        let tmpl_inp = template_input.clone(); // sync manage-section context on success
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let tid = match (*ctid).parse::<Uuid>() {
                Ok(id) => id,
                Err(_) => {
                    cstate.set(CreateState::Failed("Template ID must be a valid UUID.".to_string()));
                    return;
                }
            };
            let vid = match (*cvid).parse::<Uuid>() {
                Ok(id) => id,
                Err(_) => {
                    cstate.set(CreateState::Failed("Version ID must be a valid UUID.".to_string()));
                    return;
                }
            };

            let stages_raw = (*cstages).clone();
            if stages_raw.is_empty() {
                cstate.set(CreateState::Failed("At least one stage is required.".to_string()));
                return;
            }

            let mut stage_specs: Vec<RolloutStageSpec> = Vec::new();
            for (i, s) in stages_raw.iter().enumerate() {
                let pct = match s.target_pct.trim().parse::<i16>() {
                    Ok(p) if p > 0 && p <= 100 => p,
                    _ => {
                        cstate.set(CreateState::Failed(
                            format!("Stage {}: target percentage must be 1–100.", i + 1)
                        ));
                        return;
                    }
                };
                let depot_ids = match parse_depot_ids(&s.depot_ids_raw) {
                    Ok(ids) if !ids.is_empty() => ids,
                    Ok(_) => {
                        cstate.set(CreateState::Failed(
                            format!("Stage {}: at least one depot UUID is required.", i + 1)
                        ));
                        return;
                    }
                    Err(msg) => {
                        cstate.set(CreateState::Failed(
                            format!("Stage {}: {}", i + 1, msg)
                        ));
                        return;
                    }
                };
                let scheduled_at = if s.scheduled_at.trim().is_empty() {
                    None
                } else {
                    match parse_schedule_dt(s.scheduled_at.trim()) {
                        Some(dt) => Some(dt),
                        None => {
                            cstate.set(CreateState::Failed(
                                format!("Stage {}: invalid scheduled date.", i + 1)
                            ));
                            return;
                        }
                    }
                };
                stage_specs.push(RolloutStageSpec { target_percentage: pct, depot_ids, scheduled_at });
            }

            let notes_val = {
                let n = (*cnotes).trim().to_string();
                if n.is_empty() { None } else { Some(n) }
            };

            let body       = CreateRolloutRequest { stages: stage_specs, notes: notes_val };
            let cstate2    = cstate.clone();
            let ps2        = ps.clone();
            let plan_inp2  = plan_inp.clone();
            let tmpl_inp2  = tmpl_inp.clone();

            spawn_local(async move {
                cstate2.set(CreateState::Working);
                match ops_service::create_rollout(tid, vid, &body).await {
                    Ok(plan) => {
                        cstate2.set(CreateState::Idle);
                        // Sync the manage-section template context so activate_stage
                        // can resolve the template ID without requiring manual input.
                        tmpl_inp2.set(tid.to_string());
                        // Pre-fill the plan ID and auto-load the new plan.
                        plan_inp2.set(plan.id.to_string());
                        ps2.set(PageState::Loaded(plan));
                    }
                    Err(e) => {
                        cstate2.set(CreateState::Failed(e));
                    }
                }
            });
        })
    };

    // ── Stage builder helpers ─────────────────────────────────────────────────

    let add_stage = {
        let cs = create_stages.clone();
        Callback::from(move |_: MouseEvent| {
            let mut stages = (*cs).clone();
            stages.push(StageInput {
                target_pct:    "0".to_string(),
                depot_ids_raw: String::new(),
                scheduled_at:  String::new(),
            });
            cs.set(stages);
        })
    };

    let remove_last_stage = {
        let cs = create_stages.clone();
        Callback::from(move |_: MouseEvent| {
            let mut stages = (*cs).clone();
            if stages.len() > 1 {
                stages.pop();
                cs.set(stages);
            }
        })
    };

    // ── Load plan ─────────────────────────────────────────────────────────────

    let on_load = {
        let ti = template_input.clone();
        let pi = plan_input.clone();
        let ps = page_state.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let tid = match (*ti).parse::<Uuid>() {
                Ok(id) => id,
                Err(_) => {
                    ps.set(PageState::Error("Template ID must be a valid UUID.".to_string()));
                    return;
                }
            };
            let pid = match (*pi).parse::<Uuid>() {
                Ok(id) => id,
                Err(_) => {
                    ps.set(PageState::Error("Plan ID must be a valid UUID.".to_string()));
                    return;
                }
            };

            let ps2 = ps.clone();
            ps.set(PageState::Loading);
            spawn_local(async move {
                match ops_service::get_rollout_plan(tid, pid).await {
                    Ok(plan) => ps2.set(PageState::Loaded(plan)),
                    Err(e)   => ps2.set(PageState::Error(e)),
                }
            });
        })
    };

    // ── Activate a stage ──────────────────────────────────────────────────────

    let activate_stage = {
        let action_state = action_state.clone();
        let ti           = template_input.clone();
        let pi           = plan_input.clone();
        let ps           = page_state.clone();
        Callback::from(move |stage_id: Uuid| {
            let (tid, pid) = match precheck_activate_ids((*ti).as_str(), (*pi).as_str()) {
                Ok(ids) => ids,
                Err(msg) => {
                    action_state.set(ActionState::Failed(msg));
                    return;
                }
            };
            let ast = action_state.clone();
            let ps2 = ps.clone();
            spawn_local(async move {
                ast.set(ActionState::Working);
                match ops_service::activate_stage(tid, pid, stage_id).await {
                    Ok(_) => {
                        ast.set(ActionState::Done("Stage activated successfully.".to_string()));
                        match ops_service::get_rollout_plan(tid, pid).await {
                            Ok(plan) => ps2.set(PageState::Loaded(plan)),
                            Err(e)   => ps2.set(PageState::Error(e)),
                        }
                    }
                    Err(e) => {
                        if e.contains("[403]") {
                            ast.set(ActionState::ReauthRequired {
                                stage_id, plan_id: pid, template_id: tid,
                            });
                        } else {
                            ast.set(ActionState::Failed(e));
                        }
                    }
                }
            });
        })
    };

    // ── Reauth overlay ────────────────────────────────────────────────────────

    let reauth_overlay = if let ActionState::ReauthRequired { stage_id, .. } = &*action_state {
        let sid = *stage_id;
        let ast = action_state.clone();
        let act = activate_stage.clone();
        html! {
            <ReauthPrompt
                on_success={Callback::from(move |_| act.emit(sid))}
                on_cancel={Callback::from(move |_| ast.set(ActionState::Idle))}
            />
        }
    } else { html! {} };

    // ── Render: create form ───────────────────────────────────────────────────

    let create_err = match &*create_state {
        CreateState::Failed(e) => html! {
            <p class="form-error">{ e }</p>
        },
        _ => html! {},
    };
    let create_is_working = matches!(*create_state, CreateState::Working);

    let stage_builder = {
        let stages = (*create_stages).clone();
        let cs_add = create_stages.clone();
        let num    = stages.len();
        stages.iter().enumerate().map(|(idx, stage)| {
            let cs_pct  = cs_add.clone();
            let cs_dep  = cs_add.clone();
            let cs_sched = cs_add.clone();
            let pct_val  = stage.target_pct.clone();
            let dep_val  = stage.depot_ids_raw.clone();
            let sch_val  = stage.scheduled_at.clone();
            html! {
                <div class="stage-builder__stage" key={idx.to_string()}>
                    <h4 class="stage-builder__stage-title">
                        { format!("Stage {} of {}", idx + 1, num) }
                    </h4>
                    <label class="form-field">
                        <span>{ "Target percentage (1–100)" }</span>
                        <input type="number" min="1" max="100" step="1"
                               class="form-field__input"
                               value={pct_val}
                               oninput={{
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       let mut stages = (*cs_pct).clone();
                                       stages[idx].target_pct = el.value();
                                       cs_pct.set(stages);
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "Depot UUIDs (comma-separated)" }</span>
                        <input type="text" class="form-field__input"
                               placeholder="e.g. uuid1, uuid2, uuid3"
                               value={dep_val}
                               oninput={{
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       let mut stages = (*cs_dep).clone();
                                       stages[idx].depot_ids_raw = el.value();
                                       cs_dep.set(stages);
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "Scheduled activation (optional)" }</span>
                        <input type="datetime-local" class="form-field__input"
                               value={sch_val}
                               oninput={{
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       let mut stages = (*cs_sched).clone();
                                       stages[idx].scheduled_at = el.value();
                                       cs_sched.set(stages);
                                   })
                               }} />
                    </label>
                </div>
            }
        }).collect::<Html>()
    };

    let stages_len = (*create_stages).len();

    let create_section = if *show_create {
        html! {
            <div class="create-rollout-section">
                <h2 class="page-section__title">{ "Create Rollout Plan" }</h2>
                { create_err }
                <form onsubmit={on_create_rollout} class="create-rollout-form">
                    <label class="form-field">
                        <span>{ "Template ID" }</span>
                        <input type="text" placeholder="UUID"
                               class="form-field__input"
                               oninput={{
                                   let t = create_tmpl_id.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       t.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "Version ID (published)" }</span>
                        <input type="text" placeholder="UUID"
                               class="form-field__input"
                               oninput={{
                                   let v = create_ver_id.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       v.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "Notes (optional)" }</span>
                        <input type="text" class="form-field__input"
                               oninput={{
                                   let n = create_notes.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       n.set(el.value());
                                   })
                               }} />
                    </label>

                    <div class="stage-builder">
                        <h3 class="stage-builder__title">{ "Rollout Stages" }</h3>
                        <p class="form-hint">
                            { "Each stage activates a subset of depots. \
                               The last stage should reach 100% coverage." }
                        </p>
                        { stage_builder }
                        <div class="stage-builder__controls">
                            <button type="button" class="btn btn--secondary btn--small"
                                    onclick={add_stage}>
                                { "+ Add Stage" }
                            </button>
                            if stages_len > 1 {
                                <button type="button" class="btn btn--danger btn--small"
                                        onclick={remove_last_stage}>
                                    { "Remove Last" }
                                </button>
                            }
                        </div>
                    </div>

                    <div class="form-actions">
                        <button type="submit" class="btn btn--primary"
                                disabled={create_is_working}>
                            { if create_is_working { "Creating…" } else { "Create Rollout Plan" } }
                        </button>
                        <button type="button" class="btn btn--secondary"
                                onclick={Callback::from({
                                    let sc = show_create.clone();
                                    move |_: MouseEvent| sc.set(false)
                                })}>
                            { "Cancel" }
                        </button>
                    </div>
                </form>
            </div>
        }
    } else {
        html! {}
    };

    // ── Render: manage section ────────────────────────────────────────────────

    let is_loading = matches!(*page_state, PageState::Loading);

    let body = match &*page_state {
        PageState::Empty => html! {
            <div class="empty-state">
                <p>{ "Enter a template ID and plan ID above to load a rollout plan." }</p>
                <p class="hint">
                    { "Rollout plans are created via \"Create Rollout Plan\" above. \
                       A plan progresses through stages: e.g. 10% → 50% → 100% of depots." }
                </p>
            </div>
        },
        PageState::Loading => html! { <div class="spinner" /> },
        PageState::Error(e) => html! {
            <div class="error-state">
                <p class="error-state__message">{ e }</p>
                <button class="btn btn--secondary"
                        onclick={{
                            let ps = page_state.clone();
                            Callback::from(move |_| ps.set(PageState::Empty))
                        }}>
                    { "Reset" }
                </button>
            </div>
        },
        PageState::Loaded(plan) => {
            let act_cb     = activate_stage.clone();
            let is_working = matches!(*action_state, ActionState::Working);
            html! {
                <div class="rollout-plan">
                    <div class="rollout-plan__meta">
                        <span>{ format!("Plan: {}", plan.id) }</span>
                        <span class={format!("badge badge--{}", plan.status)}>
                            { &plan.status }
                        </span>
                        <span>{ format!("{}/{} depots", plan.current_stage, plan.total_depots) }</span>
                    </div>
                    <div class="rollout-stages">
                        { for plan.stages.iter().map(|stage| {
                            let sid        = stage.id;
                            let act_cb2    = act_cb.clone();
                            let is_pending = stage.status == "pending";
                            html! {
                                <div class="rollout-stage" key={sid.to_string()}>
                                    <div class="rollout-stage__header">
                                        <span class="rollout-stage__number">
                                            { format!("Stage {}", stage.stage_number) }
                                        </span>
                                        <span class="rollout-stage__pct">
                                            { format!("{}%", stage.target_percentage) }
                                        </span>
                                        <span class={format!("badge badge--{}", stage.status)}>
                                            { &stage.status }
                                        </span>
                                    </div>
                                    <div class="rollout-stage__detail">
                                        <span>{ format!("{} depots", stage.depot_count) }</span>
                                        if let Some(at) = stage.activated_at {
                                            <span>
                                                { format!("Activated {}", at.format("%Y-%m-%d %H:%M")) }
                                            </span>
                                        }
                                    </div>
                                    if is_pending {
                                        <button class="btn btn--primary btn--small"
                                                disabled={is_working}
                                                onclick={Callback::from(move |_| act_cb2.emit(sid))}>
                                            { if is_working { "Activating…" } else { "Activate Stage" } }
                                        </button>
                                    }
                                </div>
                            }
                        }) }
                    </div>
                </div>
            }
        }
    };

    let manage_feedback = match &*action_state {
        ActionState::Working   => html! {
            <div class="action-feedback action-feedback--working">{ "Activating stage…" }</div>
        },
        ActionState::Done(msg) => html! {
            <div class="action-feedback action-feedback--success">{ msg }</div>
        },
        ActionState::Failed(e) => html! {
            <div class="action-feedback action-feedback--error">
                { e }
                <button class="btn btn--link btn--small"
                        onclick={{
                            let ast = action_state.clone();
                            Callback::from(move |_| ast.set(ActionState::Idle))
                        }}>
                    { "Dismiss" }
                </button>
            </div>
        },
        _ => html! {}
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Rollout Manager" }</h1>
                <div class="page__actions">
                    <button class="btn btn--primary"
                            onclick={Callback::from({
                                let sc = show_create.clone();
                                move |_| sc.set(true)
                            })}>
                        { "+ Create Rollout Plan" }
                    </button>
                </div>
            </header>
            <div class="page__body">

                // ── Create rollout section ────────────────────────────────────
                { create_section }

                // ── Load + manage section ─────────────────────────────────────
                <div class="manage-rollout-section">
                    <h2 class="page-section__title">{ "Manage Existing Plan" }</h2>
                    <form onsubmit={on_load} class="inline-form">
                        <label class="form-field form-field--inline">
                            <span>{ "Template ID" }</span>
                            <input type="text" placeholder="uuid"
                                   disabled={is_loading}
                                   value={(*template_input).clone()}
                                   oninput={{
                                       let t = template_input.clone();
                                       Callback::from(move |e: InputEvent| {
                                           let i: web_sys::HtmlInputElement = e.target_unchecked_into();
                                           t.set(i.value());
                                       })
                                   }}
                                   class="form-field__input" />
                        </label>
                        <label class="form-field form-field--inline">
                            <span>{ "Plan ID" }</span>
                            <input type="text" placeholder="uuid"
                                   disabled={is_loading}
                                   value={(*plan_input).clone()}
                                   oninput={{
                                       let p = plan_input.clone();
                                       Callback::from(move |e: InputEvent| {
                                           let i: web_sys::HtmlInputElement = e.target_unchecked_into();
                                           p.set(i.value());
                                       })
                                   }}
                                   class="form-field__input" />
                        </label>
                        <button type="submit"
                                class="btn btn--secondary"
                                disabled={is_loading}>
                            { if is_loading { "Loading…" } else { "Load Plan" } }
                        </button>
                    </form>
                    { manage_feedback }
                    { body }
                </div>

            </div>
            { reauth_overlay }
        </div>
    }
}
