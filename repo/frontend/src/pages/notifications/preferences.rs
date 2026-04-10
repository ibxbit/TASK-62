/// Channel preferences & DND management page.
///
/// Allows users to configure per-channel preferences (email, push, in-app)
/// and set a Do-Not-Disturb window (start/end hours in UTC).
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::services::api;

#[derive(Clone, PartialEq, serde::Deserialize, serde::Serialize, Default)]
pub struct ChannelPreferences {
    pub email_enabled:  bool,
    pub push_enabled:   bool,
    pub in_app_enabled: bool,
    pub dnd_enabled:    bool,
    pub dnd_start_hour: u8,   // 0–23 UTC
    pub dnd_end_hour:   u8,   // 0–23 UTC
}

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(ChannelPreferences), Error(String) }

#[derive(Clone, PartialEq)]
enum SaveState { Idle, Working, Done, Failed(String) }

#[function_component(PreferencesPage)]
pub fn preferences_page() -> Html {
    let page_state = use_state(|| PageState::Loading);
    let save_state = use_state(|| SaveState::Idle);
    let prefs      = use_state(ChannelPreferences::default);

    {
        let ps  = page_state.clone();
        let prf = prefs.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match api::api_get::<ChannelPreferences>("/notifications/preferences").await {
                    Ok(p)  => { prf.set(p.clone()); ps.set(PageState::Loaded(p)); }
                    Err(e) => ps.set(PageState::Error(e)),
                }
            });
            || ()
        });
    }

    let on_save = {
        let prefs = prefs.clone();
        let ss    = save_state.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let p   = (*prefs).clone();
            let ss2 = ss.clone();
            spawn_local(async move {
                ss2.set(SaveState::Working);
                match api::api_put::<ChannelPreferences, ChannelPreferences>(
                    "/notifications/preferences", &p
                ).await {
                    Ok(_)  => ss2.set(SaveState::Done),
                    Err(e) => ss2.set(SaveState::Failed(e)),
                }
            });
        })
    };

    let toggle = |field: &'static str| {
        let p = prefs.clone();
        Callback::from(move |_: Event| {
            let mut new = (*p).clone();
            match field {
                "email"  => new.email_enabled  = !new.email_enabled,
                "push"   => new.push_enabled   = !new.push_enabled,
                "in_app" => new.in_app_enabled = !new.in_app_enabled,
                "dnd"    => new.dnd_enabled    = !new.dnd_enabled,
                _ => {}
            }
            p.set(new);
        })
    };

    let on_start_hour = {
        let p = prefs.clone();
        Callback::from(move |e: InputEvent| {
            let el: web_sys::HtmlInputElement = e.target_unchecked_into();
            let val = el.value().parse::<u8>().unwrap_or(0).min(23);
            let mut new = (*p).clone();
            new.dnd_start_hour = val;
            p.set(new);
        })
    };

    let on_end_hour = {
        let p = prefs.clone();
        Callback::from(move |e: InputEvent| {
            let el: web_sys::HtmlInputElement = e.target_unchecked_into();
            let val = el.value().parse::<u8>().unwrap_or(0).min(23);
            let mut new = (*p).clone();
            new.dnd_end_hour = val;
            p.set(new);
        })
    };

    let feedback = match &*save_state {
        SaveState::Working    => html! { <div class="action-feedback action-feedback--working">{ "Saving…" }</div> },
        SaveState::Done       => html! { <div class="action-feedback action-feedback--success">{ "Preferences saved." }</div> },
        SaveState::Failed(e)  => html! { <div class="action-feedback action-feedback--error">{ e }</div> },
        SaveState::Idle       => html! {},
    };

    let body = match &*page_state {
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        PageState::Loaded(_) => {
            let p = (*prefs).clone();
            html! {
                <form onsubmit={on_save} class="prefs-form">
                    <fieldset class="prefs-form__section">
                        <legend>{ "Channels" }</legend>
                        <label class="form-field form-field--row">
                            <input type="checkbox" checked={p.email_enabled}
                                   onchange={toggle("email")} />
                            { "Email" }
                        </label>
                        <label class="form-field form-field--row">
                            <input type="checkbox" checked={p.push_enabled}
                                   onchange={toggle("push")} />
                            { "Push notifications" }
                        </label>
                        <label class="form-field form-field--row">
                            <input type="checkbox" checked={p.in_app_enabled}
                                   onchange={toggle("in_app")} />
                            { "In-app" }
                        </label>
                    </fieldset>
                    <fieldset class="prefs-form__section">
                        <legend>{ "Do Not Disturb" }</legend>
                        <label class="form-field form-field--row">
                            <input type="checkbox" checked={p.dnd_enabled}
                                   onchange={toggle("dnd")} />
                            { "Enable DND window" }
                        </label>
                        if p.dnd_enabled {
                            <div class="prefs-form__dnd-hours">
                                <label class="form-field form-field--inline">
                                    <span>{ "Start hour (UTC)" }</span>
                                    <input type="number" min="0" max="23"
                                           value={p.dnd_start_hour.to_string()}
                                           oninput={on_start_hour}
                                           class="form-field__input form-field__input--sm" />
                                </label>
                                <label class="form-field form-field--inline">
                                    <span>{ "End hour (UTC)" }</span>
                                    <input type="number" min="0" max="23"
                                           value={p.dnd_end_hour.to_string()}
                                           oninput={on_end_hour}
                                           class="form-field__input form-field__input--sm" />
                                </label>
                            </div>
                        }
                    </fieldset>
                    <button type="submit" class="btn btn--primary"
                            disabled={matches!(&*save_state, SaveState::Working)}>
                        { if matches!(&*save_state, SaveState::Working) { "Saving…" } else { "Save Preferences" } }
                    </button>
                </form>
            }
        }
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Notification Preferences" }</h1>
            </header>
            { feedback }
            <div class="page__body">{ body }</div>
        </div>
    }
}
