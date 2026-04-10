/// Re-authentication prompt dialog.
///
/// Shown when a privileged action returns 403 with "requires re-authentication".
/// The user enters their password, which is submitted to POST /auth/reauth.
/// On success, the parent component retries the original action.
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;

use crate::services::auth_service;

// ── Component ─────────────────────────────────────────────────────────────────

#[derive(Properties, PartialEq)]
pub struct ReauthPromptProps {
    /// Called when reauth succeeds — parent should retry the blocked action.
    pub on_success: Callback<()>,
    /// Called when the user cancels the dialog.
    pub on_cancel:  Callback<()>,
}

#[function_component(ReauthPrompt)]
pub fn reauth_prompt(props: &ReauthPromptProps) -> Html {
    let password  = use_state(String::new);
    let submitting = use_state(|| false);
    let error     = use_state(|| None::<String>);

    let on_input = {
        let password = password.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            password.set(input.value());
        })
    };

    let on_submit = {
        let password    = password.clone();
        let submitting  = submitting.clone();
        let error       = error.clone();
        let on_success  = props.on_success.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let pw  = (*password).clone();
            let sub = submitting.clone();
            let err = error.clone();
            let ok  = on_success.clone();
            spawn_local(async move {
                sub.set(true);
                err.set(None);
                match auth_service::reauth(&pw).await {
                    Ok(_)  => ok.emit(()),
                    Err(e) => { err.set(Some(e)); sub.set(false); }
                }
            });
        })
    };

    html! {
        <div class="modal-overlay">
            <div class="modal reauth-prompt" role="dialog"
                 aria-label="Re-authentication required">
                <h2 class="modal__title">{ "Confirm your identity" }</h2>
                <p class="modal__body">
                    { "This action requires re-authentication within the last 10 minutes. \
                       Please enter your password to continue." }
                </p>
                <form onsubmit={on_submit}>
                    <label class="form-field">
                        <span>{ "Password" }</span>
                        <input
                            type="password"
                            class="form-field__input"
                            placeholder="Your password"
                            oninput={on_input}
                            disabled={*submitting}
                            autofocus=true
                        />
                    </label>
                    if let Some(ref e) = *error {
                        <p class="form-error">{ e }</p>
                    }
                    <div class="modal__actions">
                        <button type="submit"
                                class="btn btn--primary"
                                disabled={*submitting || password.is_empty()}>
                            if *submitting { { "Verifying…" } } else { { "Confirm" } }
                        </button>
                        <button type="button"
                                class="btn btn--ghost"
                                onclick={
                                    let cancel = props.on_cancel.clone();
                                    move |_| cancel.emit(())
                                }
                                disabled={*submitting}>
                            { "Cancel" }
                        </button>
                    </div>
                </form>
            </div>
        </div>
    }
}
