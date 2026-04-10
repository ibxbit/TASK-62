/// Operations admin: Trip calendar management page.
///
/// Lists all trip calendars.  Admins can create calendars (name, operating
/// days-of-week, validity window) and delete unused ones.  Calendars are
/// referenced by trips via `calendar_id`.
///
/// Days of week encoding: 0 = Sunday, 1 = Monday … 6 = Saturday (ISO-style
/// PostgreSQL array as used by the backend).
///
/// States:
///   PageState:  Loading | Loaded(calendars) | Error(msg)
///   FormState:  Hidden | Visible | Submitting | Failed(msg)
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::ops_service,
    types::ops::{CreateOpsCalendarRequest, OpsCalendar},
};

const DAY_LABELS: &[&str] = &["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<OpsCalendar>), Error(String) }

#[derive(Clone, PartialEq)]
enum FormState { Hidden, Visible, Submitting, Failed(String) }

#[derive(Clone, PartialEq)]
enum DeleteState { Idle, Working(Uuid), Failed(String) }

#[function_component(CalendarsAdminPage)]
pub fn calendars_admin_page() -> Html {
    let page_state   = use_state(|| PageState::Loading);
    let form_state   = use_state(|| FormState::Hidden);
    let delete_state = use_state(|| DeleteState::Idle);

    // Form fields
    let name_input       = use_state(String::new);
    let desc_input       = use_state(String::new);
    let valid_from_input = use_state(String::new);
    let valid_to_input   = use_state(String::new);
    // Selected days bitmask: index 0..6 = Sun..Sat
    let days_selected    = use_state(|| [false; 7]);

    // ── Reload list ───────────────────────────────────────────────────────────

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match ops_service::list_calendars().await {
                    Ok(cals) => ps.set(PageState::Loaded(cals)),
                    Err(e)   => ps.set(PageState::Error(e)),
                }
            });
        }
    };

    { let r = reload.clone(); use_effect_with((), move |_| { r(); || () }); }

    // ── Toggle a day selection ────────────────────────────────────────────────

    let toggle_day = {
        let days = days_selected.clone();
        move |idx: usize| {
            let days = days.clone();
            Callback::from(move |_: MouseEvent| {
                let mut arr = *days;
                arr[idx] = !arr[idx];
                days.set(arr);
            })
        }
    };

    // ── Create calendar ───────────────────────────────────────────────────────

    let on_create = {
        let fs         = form_state.clone();
        let name       = name_input.clone();
        let desc       = desc_input.clone();
        let valid_from = valid_from_input.clone();
        let valid_to   = valid_to_input.clone();
        let days       = days_selected.clone();
        let reload     = reload.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let name_val = (*name).trim().to_string();
            let from_val = (*valid_from).trim().to_string();

            if name_val.is_empty() {
                fs.set(FormState::Failed("Calendar name is required.".to_string()));
                return;
            }
            if from_val.is_empty() {
                fs.set(FormState::Failed("Valid from date is required (YYYY-MM-DD).".to_string()));
                return;
            }

            let dow: Vec<i16> = (0_usize..7)
                .filter(|&i| (*days)[i])
                .map(|i| i as i16)
                .collect();

            if dow.is_empty() {
                fs.set(FormState::Failed("Select at least one day of the week.".to_string()));
                return;
            }

            let to_val = {
                let t = (*valid_to).trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            };
            let desc_val = {
                let d = (*desc).trim().to_string();
                if d.is_empty() { None } else { Some(d) }
            };

            let body = CreateOpsCalendarRequest {
                name:         name_val,
                description:  desc_val,
                days_of_week: dow,
                valid_from:   from_val,
                valid_to:     to_val,
            };

            let fs2       = fs.clone();
            let name2     = name.clone();
            let desc2     = desc.clone();
            let vf2       = valid_from.clone();
            let vt2       = valid_to.clone();
            let days2     = days.clone();
            let reload    = reload.clone();
            spawn_local(async move {
                fs2.set(FormState::Submitting);
                match ops_service::create_calendar(&body).await {
                    Ok(_) => {
                        fs2.set(FormState::Hidden);
                        name2.set(String::new());
                        desc2.set(String::new());
                        vf2.set(String::new());
                        vt2.set(String::new());
                        days2.set([false; 7]);
                        reload();
                    }
                    Err(e) => fs2.set(FormState::Failed(e)),
                }
            });
        })
    };

    // ── Delete calendar ───────────────────────────────────────────────────────

    let make_delete = {
        let ds     = delete_state.clone();
        let reload = reload.clone();
        move |cal_id: Uuid| {
            let ds     = ds.clone();
            let reload = reload.clone();
            Callback::from(move |_: MouseEvent| {
                let ds2    = ds.clone();
                let reload = reload.clone();
                spawn_local(async move {
                    ds2.set(DeleteState::Working(cal_id));
                    match ops_service::delete_calendar(cal_id).await {
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
                { "New Calendar" }
            </button>
        },
        FormState::Visible | FormState::Submitting | FormState::Failed(_) => html! {
            <form onsubmit={on_create} class="card card--form">
                <h3 class="card__title">{ "Create Calendar" }</h3>

                if let FormState::Failed(e) = &*form_state {
                    <p class="form-field__error">{ e }</p>
                }

                <label class="form-field">
                    <span>{ "Name" }</span>
                    <input type="text" placeholder="e.g. Weekdays Summer 2025"
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
                <div class="form-field">
                    <span>{ "Operating Days" }</span>
                    <div class="day-selector">
                        { (0_usize..7).map(|i| {
                            let selected = (*days_selected)[i];
                            let cb = toggle_day(i);
                            html! {
                                <button type="button"
                                        disabled={is_submitting}
                                        class={if selected {
                                            "day-btn day-btn--active"
                                        } else {
                                            "day-btn"
                                        }}
                                        onclick={cb}>
                                    { DAY_LABELS[i] }
                                </button>
                            }
                        }).collect::<Html>() }
                    </div>
                </div>
                <label class="form-field">
                    <span>{ "Valid From" }</span>
                    <input type="date"
                           disabled={is_submitting}
                           class="form-field__input"
                           oninput={{
                               let v = valid_from_input.clone();
                               Callback::from(move |e: InputEvent| {
                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                   v.set(el.value());
                               })
                           }} />
                </label>
                <label class="form-field">
                    <span>{ "Valid To (optional)" }</span>
                    <input type="date"
                           disabled={is_submitting}
                           class="form-field__input"
                           oninput={{
                               let v = valid_to_input.clone();
                               Callback::from(move |e: InputEvent| {
                                   let el: web_sys::HtmlInputElement = e.target_unchecked_into();
                                   v.set(el.value());
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
        PageState::Loaded(cals) if cals.is_empty() => html! {
            <div class="empty-state">
                <p>{ "No calendars defined yet. Create the first one above." }</p>
            </div>
        },
        PageState::Loaded(cals) => {
            let deleting_id = match *delete_state {
                DeleteState::Working(id) => Some(id),
                _ => None,
            };
            let mk = make_delete.clone();
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Name" }</th>
                            <th>{ "Days" }</th>
                            <th>{ "Valid From" }</th>
                            <th>{ "Valid To" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for cals.iter().map(|c| {
                            let is_del = deleting_id == Some(c.id);
                            let del_cb = mk(c.id);
                            let days_str = c.days_of_week.iter()
                                .filter_map(|&d| DAY_LABELS.get(d as usize).copied())
                                .collect::<Vec<_>>()
                                .join(", ");
                            html! {
                                <tr key={c.id.to_string()}>
                                    <td>{ &c.name }</td>
                                    <td>{ days_str }</td>
                                    <td>{ c.valid_from.to_string() }</td>
                                    <td>{ c.valid_to.map(|d| d.to_string()).unwrap_or_default() }</td>
                                    <td>
                                        <button class="btn btn--danger btn--small"
                                                disabled={is_del}
                                                onclick={del_cb}>
                                            { if is_del { "Deleting…" } else { "Delete" } }
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
                <h1 class="page__title">{ "Trip Calendar Management" }</h1>
            </header>
            <div class="page__body">
                { form }
                { delete_feedback }
                { content }
            </div>
        </div>
    }
}
