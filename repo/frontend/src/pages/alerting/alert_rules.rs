/// Alerting: Alert rule subscription management page.
///
/// Lists alert rules; allows creating and deleting rules.  Supports four rule
/// types with type-specific condition fields:
///   - keyword:          keyword text + match_mode (exact/contains)
///   - topic:            topic name
///   - entity_threshold: metric_key + comparison operator + threshold value
///   - spike_detection:  metric_key + multiplier + window_minutes
///
/// Duplicate-suppression window defaults to 900 seconds (15 min).
/// Create and delete are reauth-gated on 403.
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    components::reauth_prompt::ReauthPrompt,
    services::alerting_service,
    types::alerting::{AlertRule, CreateAlertRuleRequest},
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<AlertRule>), Error(String) }

#[derive(Clone, PartialEq)]
enum ActionState {
    Idle,
    Working,
    ReauthRequired { pending: PendingAction },
    Done(String),
    Failed(String),
}

/// `Create` carries no payload — form inputs are re-read from component state
/// on reauth retry (form is still mounted during the reauth prompt).
#[derive(Clone, PartialEq)]
enum PendingAction { Create, Delete(Uuid) }

#[derive(Clone, PartialEq)]
enum FormState { Hidden, Visible, Submitting, Failed(String) }

const RULE_TYPES:   &[&str] = &["keyword", "topic", "entity_threshold", "spike_detection"];
const SEVERITIES:   &[&str] = &["info", "warning", "critical"];
const MATCH_MODES:  &[&str] = &["contains", "exact"];
const OPERATORS:    &[&str] = &["gt", "gte", "lt", "lte", "eq"];

#[function_component(AlertRulesPage)]
pub fn alert_rules_page() -> Html {
    let page_state   = use_state(|| PageState::Loading);
    let action_state = use_state(|| ActionState::Idle);
    let form_state   = use_state(|| FormState::Hidden);

    // ── Form field states ─────────────────────────────────────────────────────
    let form_name        = use_state(String::new);
    let form_rule_type   = use_state(|| RULE_TYPES[0].to_string());
    let form_severity    = use_state(|| SEVERITIES[0].to_string());
    let form_suppression = use_state(|| "900".to_string());

    // keyword conditions
    let form_keyword    = use_state(String::new);
    let form_match_mode = use_state(|| MATCH_MODES[0].to_string());

    // topic conditions
    let form_topic = use_state(String::new);

    // entity_threshold + spike_detection share metric_key
    let form_metric_key  = use_state(String::new);
    let form_threshold   = use_state(|| "0.0".to_string());
    let form_operator    = use_state(|| OPERATORS[0].to_string());
    let form_multiplier  = use_state(|| "2.0".to_string());
    let form_window_mins = use_state(|| "60".to_string());

    // ── Data reload ───────────────────────────────────────────────────────────

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match alerting_service::list_rules().await {
                    Ok(rules) => ps.set(PageState::Loaded(rules)),
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
        let fs    = form_state.clone();
        let rel   = reload.clone();
        let herr  = handle_err.clone();
        // Clone state handles for use inside the closure (originals remain accessible)
        let fname = form_name.clone();
        let frtype = form_rule_type.clone();
        let fsev  = form_severity.clone();
        let fsup  = form_suppression.clone();
        let fkw   = form_keyword.clone();
        let fmm   = form_match_mode.clone();
        let ftp   = form_topic.clone();
        let fmk   = form_metric_key.clone();
        let fthr  = form_threshold.clone();
        let fop   = form_operator.clone();
        let fmul  = form_multiplier.clone();
        let fwin  = form_window_mins.clone();
        Callback::from(move |_: ()| {
            let name = (*fname).trim().to_string();
            if name.is_empty() {
                fs.set(FormState::Failed("Rule name is required.".to_string()));
                return;
            }
            let rule_type   = (*frtype).clone();
            let severity    = (*fsev).clone();
            let suppression = (*fsup).parse::<i32>().unwrap_or(900).max(0);

            let conditions = match rule_type.as_str() {
                "keyword" => {
                    let kw = (*fkw).trim().to_string();
                    if kw.is_empty() {
                        fs.set(FormState::Failed("Keyword is required.".to_string()));
                        return;
                    }
                    serde_json::json!({ "keyword": kw, "match_mode": *fmm })
                }
                "topic" => {
                    let tp = (*ftp).trim().to_string();
                    if tp.is_empty() {
                        fs.set(FormState::Failed("Topic is required.".to_string()));
                        return;
                    }
                    serde_json::json!({ "topic": tp })
                }
                "entity_threshold" => {
                    let mk = (*fmk).trim().to_string();
                    if mk.is_empty() {
                        fs.set(FormState::Failed("Metric key is required.".to_string()));
                        return;
                    }
                    let thr = match (*fthr).parse::<f64>() {
                        Ok(v)  => v,
                        Err(_) => {
                            fs.set(FormState::Failed("Threshold must be a number.".to_string()));
                            return;
                        }
                    };
                    serde_json::json!({ "metric_key": mk, "threshold": thr, "operator": *fop })
                }
                "spike_detection" => {
                    let mk = (*fmk).trim().to_string();
                    if mk.is_empty() {
                        fs.set(FormState::Failed("Metric key is required.".to_string()));
                        return;
                    }
                    let mul = (*fmul).parse::<f64>().unwrap_or(2.0).max(0.1);
                    let win = (*fwin).parse::<i32>().unwrap_or(60).max(1);
                    serde_json::json!({ "metric_key": mk, "multiplier": mul, "window_minutes": win })
                }
                _ => serde_json::json!({}),
            };

            let body = CreateAlertRuleRequest {
                name,
                rule_type,
                severity,
                conditions,
                duplicate_suppression_window_secs: Some(suppression),
            };

            let ast2  = ast.clone();
            let fs2   = fs.clone();
            let rel2  = rel.clone();
            let herr2 = herr.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                fs2.set(FormState::Submitting);
                match alerting_service::create_rule(&body).await {
                    Ok(_)  => {
                        ast2.set(ActionState::Done("Rule created".into()));
                        fs2.set(FormState::Hidden);
                        rel2();
                    }
                    Err(e) => {
                        fs2.set(FormState::Visible);
                        herr2(e, PendingAction::Create);
                    }
                }
            });
        })
    };

    let on_submit = {
        let dc = do_create.clone();
        Callback::from(move |e: SubmitEvent| { e.prevent_default(); dc.emit(()); })
    };

    // ── Delete ────────────────────────────────────────────────────────────────

    let delete_rule = {
        let ast  = action_state.clone();
        let rel  = reload.clone();
        let herr = handle_err.clone();
        Callback::from(move |rid: Uuid| {
            let ast2  = ast.clone();
            let rel2  = rel.clone();
            let herr2 = herr.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match alerting_service::delete_rule(rid).await {
                    Ok(_)  => { ast2.set(ActionState::Done("Rule deleted".into())); rel2(); }
                    Err(e) => herr2(e, PendingAction::Delete(rid)),
                }
            });
        })
    };

    // ── Table content ─────────────────────────────────────────────────────────

    let content = match &*page_state {
        PageState::Loading => html! {
            <div class="loading-state"><div class="spinner"/></div>
        },
        PageState::Error(e) => html! {
            <p class="error-state__message">{ e }</p>
        },
        PageState::Loaded(rules) if rules.is_empty() => html! {
            <div class="empty-state"><p>{ "No alert rules configured." }</p></div>
        },
        PageState::Loaded(rules) => {
            let del = delete_rule.clone();
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Name" }</th>
                            <th>{ "Type" }</th>
                            <th>{ "Severity" }</th>
                            <th>{ "Conditions" }</th>
                            <th>{ "Suppression (s)" }</th>
                            <th>{ "Active" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for rules.iter().map(|r| {
                            let rid  = r.id;
                            let del2 = del.clone();
                            html! {
                                <tr key={rid.to_string()}>
                                    <td>{ &r.name }</td>
                                    <td>{ r.rule_type_label() }</td>
                                    <td>
                                        <span class={format!("badge badge--{}", r.severity)}>
                                            { &r.severity }
                                        </span>
                                    </td>
                                    <td class="mono text-sm">{ r.conditions_summary() }</td>
                                    <td>{ r.duplicate_suppression_window_secs }</td>
                                    <td>{ if r.is_active { "Yes" } else { "No" } }</td>
                                    <td class="action-cell">
                                        <button class="btn btn--small btn--danger"
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

    let show_form = !matches!(&*form_state, FormState::Hidden);
    let is_sub    = matches!(&*form_state, FormState::Submitting);
    let form_err  = if let FormState::Failed(e) = &*form_state {
        html! { <p class="form-error">{ e }</p> }
    } else { html! {} };

    // Current rule type drives which condition fields are rendered.
    let rt_str = (*form_rule_type).clone();

    let condition_fields = match rt_str.as_str() {
        "keyword" => html! {
            <>
                <label class="form-field">
                    <span>{ "Keyword" }</span>
                    <input type="text" class="form-field__input"
                           placeholder="e.g. delay"
                           oninput={{
                               let k = form_keyword.clone();
                               Callback::from(move |e: InputEvent| {
                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                   k.set(el.value());
                               })
                           }} />
                </label>
                <label class="form-field">
                    <span>{ "Match mode" }</span>
                    <select class="form-field__input"
                            onchange={{
                                let mm = form_match_mode.clone();
                                Callback::from(move |e: Event| {
                                    let el: web_sys::HtmlSelectElement = e.target_unchecked_into();
                                    mm.set(el.value());
                                })
                            }}>
                        { for MATCH_MODES.iter().map(|m| html! {
                            <option value={*m}>{ *m }</option>
                        }) }
                    </select>
                </label>
            </>
        },
        "topic" => html! {
            <label class="form-field">
                <span>{ "Topic" }</span>
                <input type="text" class="form-field__input"
                       placeholder="e.g. route.delay"
                       oninput={{
                           let t = form_topic.clone();
                           Callback::from(move |e: InputEvent| {
                               let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                               t.set(el.value());
                           })
                       }} />
            </label>
        },
        "entity_threshold" => html! {
            <>
                <label class="form-field">
                    <span>{ "Metric key" }</span>
                    <input type="text" class="form-field__input"
                           placeholder="e.g. on_time_rate"
                           oninput={{
                               let mk = form_metric_key.clone();
                               Callback::from(move |e: InputEvent| {
                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                   mk.set(el.value());
                               })
                           }} />
                </label>
                <label class="form-field">
                    <span>{ "Operator" }</span>
                    <select class="form-field__input"
                            onchange={{
                                let op = form_operator.clone();
                                Callback::from(move |e: Event| {
                                    let el: web_sys::HtmlSelectElement = e.target_unchecked_into();
                                    op.set(el.value());
                                })
                            }}>
                        { for OPERATORS.iter().map(|o| html! {
                            <option value={*o}>{ *o }</option>
                        }) }
                    </select>
                </label>
                <label class="form-field">
                    <span>{ "Threshold" }</span>
                    <input type="number" step="0.01" class="form-field__input"
                           value={(*form_threshold).clone()}
                           oninput={{
                               let thr = form_threshold.clone();
                               Callback::from(move |e: InputEvent| {
                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                   thr.set(el.value());
                               })
                           }} />
                </label>
            </>
        },
        "spike_detection" => html! {
            <>
                <label class="form-field">
                    <span>{ "Metric key" }</span>
                    <input type="text" class="form-field__input"
                           placeholder="e.g. incident_count"
                           oninput={{
                               let mk = form_metric_key.clone();
                               Callback::from(move |e: InputEvent| {
                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                   mk.set(el.value());
                               })
                           }} />
                </label>
                <label class="form-field">
                    <span>{ "Multiplier (×)" }</span>
                    <input type="number" min="0.1" step="0.1" class="form-field__input"
                           value={(*form_multiplier).clone()}
                           oninput={{
                               let mul = form_multiplier.clone();
                               Callback::from(move |e: InputEvent| {
                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                   mul.set(el.value());
                               })
                           }} />
                </label>
                <label class="form-field">
                    <span>{ "Window (minutes)" }</span>
                    <input type="number" min="1" step="1" class="form-field__input"
                           value={(*form_window_mins).clone()}
                           oninput={{
                               let win = form_window_mins.clone();
                               Callback::from(move |e: InputEvent| {
                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                   win.set(el.value());
                               })
                           }} />
                </label>
            </>
        },
        _ => html! {},
    };

    let form_modal = if show_form {
        let cancel = {
            let fs = form_state.clone();
            Callback::from(move |_: MouseEvent| fs.set(FormState::Hidden))
        };
        html! {
            <div class="modal-overlay">
                <div class="modal">
                    <h2 class="modal__title">{ "Create Alert Rule" }</h2>
                    { form_err }
                    <form onsubmit={on_submit} class="modal__form">
                        <label class="form-field">
                            <span>{ "Rule name" }</span>
                            <input type="text" class="form-field__input"
                                   placeholder="e.g. High Incident Rate"
                                   oninput={{
                                       let n = form_name.clone();
                                       Callback::from(move |e: InputEvent| {
                                           let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                           n.set(el.value());
                                       })
                                   }} />
                        </label>
                        <label class="form-field">
                            <span>{ "Rule type" }</span>
                            <select class="form-field__input"
                                    onchange={{
                                        let rt = form_rule_type.clone();
                                        Callback::from(move |e: Event| {
                                            let el: web_sys::HtmlSelectElement = e.target_unchecked_into();
                                            rt.set(el.value());
                                        })
                                    }}>
                                { for RULE_TYPES.iter().map(|t| html! {
                                    <option value={*t}>{ *t }</option>
                                }) }
                            </select>
                        </label>
                        <label class="form-field">
                            <span>{ "Severity" }</span>
                            <select class="form-field__input"
                                    onchange={{
                                        let sv = form_severity.clone();
                                        Callback::from(move |e: Event| {
                                            let el: web_sys::HtmlSelectElement = e.target_unchecked_into();
                                            sv.set(el.value());
                                        })
                                    }}>
                                { for SEVERITIES.iter().map(|s| html! {
                                    <option value={*s}>{ *s }</option>
                                }) }
                            </select>
                        </label>
                        { condition_fields }
                        <label class="form-field">
                            <span>{ "Duplicate suppression (seconds)" }</span>
                            <input type="number" min="0" step="1" class="form-field__input"
                                   value={(*form_suppression).clone()}
                                   oninput={{
                                       let sup = form_suppression.clone();
                                       Callback::from(move |e: InputEvent| {
                                           let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                           sup.set(el.value());
                                       })
                                   }} />
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
    } else {
        html! {}
    };

    // ── Feedback / reauth ─────────────────────────────────────────────────────

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
        _ => html! {},
    };

    let reauth_overlay = if let ActionState::ReauthRequired { pending } = &*action_state {
        let pending   = pending.clone();
        let del_cb    = delete_rule.clone();
        let create_cb = do_create.clone();
        html! {
            <ReauthPrompt
                on_success={Callback::from(move |_| {
                    match &pending {
                        PendingAction::Create      => create_cb.emit(()),
                        PendingAction::Delete(id)  => del_cb.emit(*id),
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
                <h1 class="page__title">{ "Alert Rules" }</h1>
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
                { feedback }
                { content }
            </div>
            { form_modal }
            { reauth_overlay }
        </div>
    }
}
