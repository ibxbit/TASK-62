/// Login page.
///
/// Handles username/password authentication, stores the token in localStorage
/// (via AuthStore), and loads the session profile before redirecting.
/// Loading / submitting / error states are all rendered explicitly.
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::{
    pages::Route,
    services::auth_service,
    store::auth_store::{AuthAction, AuthContext},
};

// ── State machine ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum LoginState {
    Idle,
    Submitting,
    Error(String),
}

// ── Component ─────────────────────────────────────────────────────────────────

#[function_component(LoginPage)]
pub fn login_page() -> Html {
    let auth     = use_context::<AuthContext>().expect("AuthContext missing");
    let navigator = use_navigator().expect("Navigator missing");

    let username = use_state(String::new);
    let password = use_state(String::new);
    let state    = use_state(|| LoginState::Idle);

    // Redirect if already authenticated
    {
        let auth = auth.clone();
        let nav  = navigator.clone();
        use_effect_with(auth.is_authenticated(), move |authed| {
            if *authed { nav.push(&Route::Inbox); }
            || ()
        });
    }

    let on_username = {
        let username = username.clone();
        Callback::from(move |e: InputEvent| {
            let inp: HtmlInputElement = e.target_unchecked_into();
            username.set(inp.value());
        })
    };

    let on_password = {
        let password = password.clone();
        Callback::from(move |e: InputEvent| {
            let inp: HtmlInputElement = e.target_unchecked_into();
            password.set(inp.value());
        })
    };

    let on_submit = {
        let username  = username.clone();
        let password  = password.clone();
        let state_h   = state.clone();
        let auth      = auth.clone();
        let nav       = navigator.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let u    = (*username).clone();
            let pw   = (*password).clone();
            let stat = state_h.clone();
            let auth = auth.clone();
            let nav  = nav.clone();
            spawn_local(async move {
                stat.set(LoginState::Submitting);
                match auth_service::login(&u, &pw).await {
                    Ok(resp) => {
                        auth.dispatch(AuthAction::SetToken(resp.token));
                        // Load full session profile
                        if let Ok(session) = auth_service::get_session().await {
                            auth.dispatch(AuthAction::SetSession(session));
                        }
                        nav.push(&Route::Inbox);
                    }
                    Err(e) => stat.set(LoginState::Error(e)),
                }
            });
        })
    };

    let is_submitting = matches!(*state, LoginState::Submitting);

    html! {
        <div class="login-page">
            <div class="login-card">
                <h1 class="login-card__title">{ "TransitOps" }</h1>
                <p class="login-card__subtitle">{ "Backoffice Portal" }</p>

                <form onsubmit={on_submit} class="login-form">
                    <label class="form-field">
                        <span class="form-field__label">{ "Username" }</span>
                        <input
                            type="text"
                            class="form-field__input"
                            placeholder="username"
                            oninput={on_username}
                            disabled={is_submitting}
                            autocomplete="username"
                            autofocus=true
                        />
                    </label>

                    <label class="form-field">
                        <span class="form-field__label">{ "Password" }</span>
                        <input
                            type="password"
                            class="form-field__input"
                            placeholder="••••••••"
                            oninput={on_password}
                            disabled={is_submitting}
                            autocomplete="current-password"
                        />
                    </label>

                    if let LoginState::Error(ref msg) = *state {
                        <p class="form-error">{ msg }</p>
                    }

                    <button
                        type="submit"
                        class="btn btn--primary btn--full-width"
                        disabled={is_submitting || username.is_empty() || password.is_empty()}>
                        if is_submitting { { "Signing in…" } } else { { "Sign in" } }
                    </button>
                </form>
            </div>
        </div>
    }
}
