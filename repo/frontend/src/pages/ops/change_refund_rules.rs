/// Ops admin: Change policy and refund policy configuration.
///
/// Two sections on one page:
///   - Change Policies  — rules governing ticket modification fees and windows
///   - Refund Policies  — rules governing refund eligibility and amounts
///
/// Both sections support list + create + delete.  Mutations are not
/// reauth-gated here (admin-only page rarely encounters stale sessions),
/// but errors surface as inline feedback.  Admin-only — guarded at the router.
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::ops_service,
    types::ops::{
        ChangePolicy, CreateChangePolicyRequest,
        CreateRefundPolicyRequest, RefundPolicy,
    },
};

// ── Shared state types ────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum SectionState<T: Clone + PartialEq> { Loading, Loaded(Vec<T>), Error(String) }

#[derive(Clone, PartialEq)]
enum FormState { Hidden, Visible, Submitting, Failed(String) }

// ── Component ─────────────────────────────────────────────────────────────────

#[function_component(ChangeRefundRulesPage)]
pub fn change_refund_rules_page() -> Html {
    // ── Change policy state ───────────────────────────────────────────────────
    let change_state = use_state(|| SectionState::Loading);
    let change_form  = use_state(|| FormState::Hidden);
    let cp_name      = use_state(String::new);
    let cp_desc      = use_state(String::new);
    let cp_fee       = use_state(|| "0.00".to_string());
    let cp_window    = use_state(|| "24".to_string());

    // ── Refund policy state ───────────────────────────────────────────────────
    let refund_state  = use_state(|| SectionState::Loading);
    let refund_form   = use_state(|| FormState::Hidden);
    let rp_name       = use_state(String::new);
    let rp_desc       = use_state(String::new);
    let rp_pct        = use_state(|| "100.0".to_string());
    let rp_window     = use_state(|| "24".to_string());
    let rp_no_show    = use_state(|| "0.00".to_string());

    // ── Reload helpers ────────────────────────────────────────────────────────

    let reload_change = {
        let cs = change_state.clone();
        move || {
            let cs = cs.clone();
            spawn_local(async move {
                match ops_service::list_change_policies().await {
                    Ok(items) => cs.set(SectionState::Loaded(items)),
                    Err(e)    => cs.set(SectionState::Error(e)),
                }
            });
        }
    };

    let reload_refund = {
        let rs = refund_state.clone();
        move || {
            let rs = rs.clone();
            spawn_local(async move {
                match ops_service::list_refund_policies().await {
                    Ok(items) => rs.set(SectionState::Loaded(items)),
                    Err(e)    => rs.set(SectionState::Error(e)),
                }
            });
        }
    };

    {
        let rc = reload_change.clone();
        let rr = reload_refund.clone();
        use_effect_with((), move |_| { rc(); rr(); || () });
    }

    // ── Create change policy ──────────────────────────────────────────────────

    let on_create_change = {
        let cf   = change_form.clone();
        let rel  = reload_change.clone();
        let name = cp_name.clone();
        let desc = cp_desc.clone();
        let fee  = cp_fee.clone();
        let win  = cp_window.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let name_val = (*name).trim().to_string();
            if name_val.is_empty() {
                cf.set(FormState::Failed("Policy name is required.".to_string()));
                return;
            }
            let fee_val = match (*fee).parse::<f64>() {
                Ok(f) if f >= 0.0 => f,
                _ => {
                    cf.set(FormState::Failed("Change fee must be a non-negative number.".to_string()));
                    return;
                }
            };
            let win_val = match (*win).parse::<i32>() {
                Ok(h) if h > 0 => h,
                _ => {
                    cf.set(FormState::Failed("Change window must be a positive integer.".to_string()));
                    return;
                }
            };
            let body = CreateChangePolicyRequest {
                name:                name_val,
                description:         {
                    let d = (*desc).trim().to_string();
                    if d.is_empty() { None } else { Some(d) }
                },
                change_fee:          fee_val,
                change_window_hours: win_val,
            };
            let cf2  = cf.clone();
            let rel2 = rel.clone();
            spawn_local(async move {
                cf2.set(FormState::Submitting);
                match ops_service::create_change_policy(&body).await {
                    Ok(_)  => { cf2.set(FormState::Hidden); rel2(); }
                    Err(e) => cf2.set(FormState::Failed(e)),
                }
            });
        })
    };

    // ── Delete change policy ──────────────────────────────────────────────────

    let on_delete_change = {
        let rel = reload_change.clone();
        Callback::from(move |id: Uuid| {
            let rel = rel.clone();
            spawn_local(async move {
                let _ = ops_service::delete_change_policy(id).await;
                rel();
            });
        })
    };

    // ── Create refund policy ──────────────────────────────────────────────────

    let on_create_refund = {
        let rf   = refund_form.clone();
        let rel  = reload_refund.clone();
        let name = rp_name.clone();
        let desc = rp_desc.clone();
        let pct  = rp_pct.clone();
        let win  = rp_window.clone();
        let nsf  = rp_no_show.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let name_val = (*name).trim().to_string();
            if name_val.is_empty() {
                rf.set(FormState::Failed("Policy name is required.".to_string()));
                return;
            }
            let pct_val = match (*pct).parse::<f64>() {
                Ok(p) if p >= 0.0 && p <= 100.0 => p,
                _ => {
                    rf.set(FormState::Failed("Refund percentage must be 0–100.".to_string()));
                    return;
                }
            };
            let win_val = match (*win).parse::<i32>() {
                Ok(h) if h > 0 => h,
                _ => {
                    rf.set(FormState::Failed("Refund window must be a positive integer.".to_string()));
                    return;
                }
            };
            let nsf_val = (*nsf).parse::<f64>().unwrap_or(0.0).max(0.0);
            let body = CreateRefundPolicyRequest {
                name:                 name_val,
                description:          {
                    let d = (*desc).trim().to_string();
                    if d.is_empty() { None } else { Some(d) }
                },
                refund_percentage:    pct_val,
                refund_window_hours:  win_val,
                no_show_fee:          nsf_val,
            };
            let rf2  = rf.clone();
            let rel2 = rel.clone();
            spawn_local(async move {
                rf2.set(FormState::Submitting);
                match ops_service::create_refund_policy(&body).await {
                    Ok(_)  => { rf2.set(FormState::Hidden); rel2(); }
                    Err(e) => rf2.set(FormState::Failed(e)),
                }
            });
        })
    };

    // ── Delete refund policy ──────────────────────────────────────────────────

    let on_delete_refund = {
        let rel = reload_refund.clone();
        Callback::from(move |id: Uuid| {
            let rel = rel.clone();
            spawn_local(async move {
                let _ = ops_service::delete_refund_policy(id).await;
                rel();
            });
        })
    };

    // ── Change policies section ───────────────────────────────────────────────

    let change_content = match &*change_state {
        SectionState::Loading => html! {
            <div class="loading-state"><div class="spinner"/></div>
        },
        SectionState::Error(e) => html! {
            <p class="error-state__message">{ e }</p>
        },
        SectionState::Loaded(items) if items.is_empty() => html! {
            <div class="empty-state"><p>{ "No change policies configured." }</p></div>
        },
        SectionState::Loaded(items) => {
            let del = on_delete_change.clone();
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Name" }</th>
                            <th>{ "Change Fee" }</th>
                            <th>{ "Window (h)" }</th>
                            <th>{ "Active" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for items.iter().map(|p| {
                            let pid  = p.id;
                            let del2 = del.clone();
                            html! {
                                <tr key={pid.to_string()}>
                                    <td>{ &p.name }</td>
                                    <td>{ format!("{:.2}", p.change_fee) }</td>
                                    <td>{ p.change_window_hours }</td>
                                    <td>{ if p.is_active { "Yes" } else { "No" } }</td>
                                    <td class="action-cell">
                                        <button class="btn btn--small btn--danger"
                                                onclick={Callback::from(move |_| del2.emit(pid))}>
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

    let show_change_form  = !matches!(&*change_form, FormState::Hidden);
    let change_form_is_sub = matches!(&*change_form, FormState::Submitting);
    let change_form_err   = if let FormState::Failed(e) = &*change_form {
        html! { <p class="form-error">{ e }</p> }
    } else { html! {} };

    let change_form_section = if show_change_form {
        html! {
            <div class="section-form section-form--inline">
                { change_form_err }
                <form onsubmit={on_create_change} class="inline-form inline-form--stacked">
                    <label class="form-field">
                        <span>{ "Policy name" }</span>
                        <input type="text" class="form-field__input"
                               placeholder="e.g. Standard Change Policy"
                               oninput={{
                                   let n = cp_name.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       n.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "Description (optional)" }</span>
                        <input type="text" class="form-field__input"
                               oninput={{
                                   let d = cp_desc.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       d.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "Change fee" }</span>
                        <input type="number" min="0" step="0.01"
                               class="form-field__input"
                               value={(*cp_fee).clone()}
                               oninput={{
                                   let f = cp_fee.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       f.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "Change window (hours)" }</span>
                        <input type="number" min="1" step="1"
                               class="form-field__input"
                               value={(*cp_window).clone()}
                               oninput={{
                                   let w = cp_window.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       w.set(el.value());
                                   })
                               }} />
                    </label>
                    <div class="form-actions">
                        <button type="submit" class="btn btn--primary"
                                disabled={change_form_is_sub}>
                            { if change_form_is_sub { "Creating…" } else { "Create Policy" } }
                        </button>
                        <button type="button" class="btn btn--secondary"
                                onclick={Callback::from({
                                    let cf = change_form.clone();
                                    move |_: MouseEvent| cf.set(FormState::Hidden)
                                })}>
                            { "Cancel" }
                        </button>
                    </div>
                </form>
            </div>
        }
    } else { html! {} };

    // ── Refund policies section ───────────────────────────────────────────────

    let refund_content = match &*refund_state {
        SectionState::Loading => html! {
            <div class="loading-state"><div class="spinner"/></div>
        },
        SectionState::Error(e) => html! {
            <p class="error-state__message">{ e }</p>
        },
        SectionState::Loaded(items) if items.is_empty() => html! {
            <div class="empty-state"><p>{ "No refund policies configured." }</p></div>
        },
        SectionState::Loaded(items) => {
            let del = on_delete_refund.clone();
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Name" }</th>
                            <th>{ "Refund %" }</th>
                            <th>{ "Window (h)" }</th>
                            <th>{ "No-show Fee" }</th>
                            <th>{ "Active" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for items.iter().map(|p| {
                            let pid  = p.id;
                            let del2 = del.clone();
                            html! {
                                <tr key={pid.to_string()}>
                                    <td>{ &p.name }</td>
                                    <td>{ format!("{:.1}%", p.refund_percentage) }</td>
                                    <td>{ p.refund_window_hours }</td>
                                    <td>{ format!("{:.2}", p.no_show_fee) }</td>
                                    <td>{ if p.is_active { "Yes" } else { "No" } }</td>
                                    <td class="action-cell">
                                        <button class="btn btn--small btn--danger"
                                                onclick={Callback::from(move |_| del2.emit(pid))}>
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

    let show_refund_form   = !matches!(&*refund_form, FormState::Hidden);
    let refund_form_is_sub = matches!(&*refund_form, FormState::Submitting);
    let refund_form_err    = if let FormState::Failed(e) = &*refund_form {
        html! { <p class="form-error">{ e }</p> }
    } else { html! {} };

    let refund_form_section = if show_refund_form {
        html! {
            <div class="section-form section-form--inline">
                { refund_form_err }
                <form onsubmit={on_create_refund} class="inline-form inline-form--stacked">
                    <label class="form-field">
                        <span>{ "Policy name" }</span>
                        <input type="text" class="form-field__input"
                               placeholder="e.g. Standard Refund Policy"
                               oninput={{
                                   let n = rp_name.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       n.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "Description (optional)" }</span>
                        <input type="text" class="form-field__input"
                               oninput={{
                                   let d = rp_desc.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       d.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "Refund percentage (0–100)" }</span>
                        <input type="number" min="0" max="100" step="0.1"
                               class="form-field__input"
                               value={(*rp_pct).clone()}
                               oninput={{
                                   let p = rp_pct.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       p.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "Refund window (hours)" }</span>
                        <input type="number" min="1" step="1"
                               class="form-field__input"
                               value={(*rp_window).clone()}
                               oninput={{
                                   let w = rp_window.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       w.set(el.value());
                                   })
                               }} />
                    </label>
                    <label class="form-field">
                        <span>{ "No-show fee" }</span>
                        <input type="number" min="0" step="0.01"
                               class="form-field__input"
                               value={(*rp_no_show).clone()}
                               oninput={{
                                   let f = rp_no_show.clone();
                                   Callback::from(move |e: InputEvent| {
                                       let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                       f.set(el.value());
                                   })
                               }} />
                    </label>
                    <div class="form-actions">
                        <button type="submit" class="btn btn--primary"
                                disabled={refund_form_is_sub}>
                            { if refund_form_is_sub { "Creating…" } else { "Create Policy" } }
                        </button>
                        <button type="button" class="btn btn--secondary"
                                onclick={Callback::from({
                                    let rf = refund_form.clone();
                                    move |_: MouseEvent| rf.set(FormState::Hidden)
                                })}>
                            { "Cancel" }
                        </button>
                    </div>
                </form>
            </div>
        }
    } else { html! {} };

    // ── Full page render ──────────────────────────────────────────────────────

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Change & Refund Rules" }</h1>
            </header>
            <div class="page__body">

                // ── Change Policies ───────────────────────────────────────────
                <section class="page-section">
                    <div class="page-section__header">
                        <h2 class="page-section__title">{ "Change Policies" }</h2>
                        <button class="btn btn--primary btn--small"
                                onclick={Callback::from({
                                    let cf = change_form.clone();
                                    move |_| cf.set(FormState::Visible)
                                })}>
                            { "+ Add Change Policy" }
                        </button>
                    </div>
                    { change_form_section }
                    { change_content }
                </section>

                // ── Refund Policies ───────────────────────────────────────────
                <section class="page-section">
                    <div class="page-section__header">
                        <h2 class="page-section__title">{ "Refund Policies" }</h2>
                        <button class="btn btn--primary btn--small"
                                onclick={Callback::from({
                                    let rf = refund_form.clone();
                                    move |_| rf.set(FormState::Visible)
                                })}>
                            { "+ Add Refund Policy" }
                        </button>
                    </div>
                    { refund_form_section }
                    { refund_content }
                </section>

            </div>
        </div>
    }
}
