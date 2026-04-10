/// Authentication / session state store.
///
/// Persists the session token to `localStorage` so the user stays logged in
/// across page refreshes (offline-first: token is cached locally).
///
/// On application startup the `AuthProvider` attempts to restore the full
/// session profile by calling `GET /auth/session`.  If the stored token is
/// expired or invalid the provider clears it (Logout) so the user is
/// redirected to `/login` cleanly.  Guards show a loading screen during the
/// brief restore window to prevent redirect flicker.
///
/// The `AuthContext` wraps (SessionInfo, token, dispatch) and is available to
/// any component in the tree via `use_context::<AuthContext>()`.
use std::rc::Rc;

use wasm_bindgen_futures::spawn_local;
use web_sys::window;
use yew::prelude::*;

use crate::{
    services::auth_service,
    types::auth::SessionInfo,
};

pub const TOKEN_KEY: &str = "transitops_token";

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug, Default)]
pub struct AuthState {
    pub token:   Option<String>,
    pub session: Option<SessionInfo>,
    pub loading: bool,
}

impl AuthState {
    pub fn is_authenticated(&self) -> bool {
        self.token.is_some() && self.session.is_some()
    }

    pub fn role(&self) -> &str {
        self.session.as_ref().map(|s| s.role.as_str()).unwrap_or("")
    }
}

// ── Actions ───────────────────────────────────────────────────────────────────

pub enum AuthAction {
    SetToken(String),
    SetSession(SessionInfo),
    SetLoading(bool),
    Logout,
}

// ── Reducer ───────────────────────────────────────────────────────────────────

impl Reducible for AuthState {
    type Action = AuthAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            AuthAction::SetToken(token) => {
                // Persist to localStorage for offline-first usage
                if let Some(storage) = window().and_then(|w| w.local_storage().ok()).flatten() {
                    let _ = storage.set_item(TOKEN_KEY, &token);
                }
                Rc::new(AuthState {
                    token:   Some(token),
                    session: self.session.clone(),
                    loading: self.loading,
                })
            }
            AuthAction::SetSession(session) => Rc::new(AuthState {
                token:   self.token.clone(),
                session: Some(session),
                loading: self.loading,
            }),
            AuthAction::SetLoading(loading) => Rc::new(AuthState {
                loading,
                ..(*self).clone()
            }),
            AuthAction::Logout => {
                if let Some(storage) = window().and_then(|w| w.local_storage().ok()).flatten() {
                    let _ = storage.remove_item(TOKEN_KEY);
                }
                Rc::new(AuthState::default())
            }
        }
    }
}

// ── Context type ──────────────────────────────────────────────────────────────

pub type AuthContext = UseReducerHandle<AuthState>;

// ── Load persisted token from localStorage ────────────────────────────────────

pub fn load_persisted_token() -> Option<String> {
    window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
        .and_then(|s| s.get_item(TOKEN_KEY).ok())
        .flatten()
}

// ── Provider ──────────────────────────────────────────────────────────────────

#[derive(Properties, PartialEq)]
pub struct AuthProviderProps {
    pub children: Children,
}

/// Wraps the application tree and provides `AuthContext` to all descendants.
///
/// On first mount:
///   1. Initialises with `loading: true` so guards show a loading screen.
///   2. Checks localStorage for a persisted token.
///   3. If found: dispatches `SetToken`, then calls `GET /auth/session`.
///      - On success: dispatches `SetSession` + `SetLoading(false)` → guards pass.
///      - On failure: dispatches `Logout` (clears token + resets state) →
///        guards redirect to `/login`.
///   4. If no token: dispatches `SetLoading(false)` → guards redirect to `/login`.
#[function_component(AuthProvider)]
pub fn auth_provider(props: &AuthProviderProps) -> Html {
    // Start in loading state so route guards don't flicker to /login before
    // we've had a chance to verify the persisted token.
    let state = use_reducer(|| AuthState {
        loading: true,
        ..AuthState::default()
    });

    {
        let state = state.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match load_persisted_token() {
                    None => {
                        // No stored token — immediately mark load complete so
                        // guards redirect to /login.
                        state.dispatch(AuthAction::SetLoading(false));
                    }
                    Some(token) => {
                        // Restore the token so bearer_header() in api.rs can
                        // send it with the upcoming GET /auth/session call.
                        state.dispatch(AuthAction::SetToken(token));

                        // Verify the token is still valid with the server.
                        match auth_service::get_session().await {
                            Ok(session) => {
                                state.dispatch(AuthAction::SetSession(session));
                                state.dispatch(AuthAction::SetLoading(false));
                            }
                            Err(_) => {
                                // Token expired, revoked, or server error —
                                // clear everything; Logout resets to Default
                                // which has loading: false.
                                state.dispatch(AuthAction::Logout);
                            }
                        }
                    }
                }
            });
            || ()
        });
    }

    html! {
        <ContextProvider<AuthContext> context={state}>
            { for props.children.iter() }
        </ContextProvider<AuthContext>>
    }
}
