/// Role-aware route guard.
///
/// Wraps page content and redirects unauthenticated users to /login.
/// Optionally checks that the current role satisfies a predicate; if not,
/// renders a "403 Forbidden" banner instead of the page content.
use yew::prelude::*;
use yew_router::prelude::*;

use crate::{
    pages::Route,
    store::auth_store::AuthContext,
    types::auth::SessionInfo,
};

// ── AuthGuard — authentication only ──────────────────────────────────────────

/// Renders `children` only when the user is authenticated.
/// Redirects to `/login` otherwise.
#[derive(Properties, PartialEq)]
pub struct AuthGuardProps {
    pub children: Children,
}

#[function_component(AuthGuard)]
pub fn auth_guard(props: &AuthGuardProps) -> Html {
    let auth = use_context::<AuthContext>().expect("AuthContext missing");

    if auth.loading {
        return html! { <div class="loading-screen">{ "Loading…" }</div> };
    }

    if !auth.is_authenticated() {
        return html! { <Redirect<Route> to={Route::Login} /> };
    }

    html! { { for props.children.iter() } }
}

// ── RoleGuard — role-predicate check ─────────────────────────────────────────

/// Renders `children` only when the user is authenticated AND the role
/// satisfies `allowed`.  Shows a 403 message otherwise.
#[derive(Properties, PartialEq)]
pub struct RoleGuardProps {
    pub children: Children,
    /// Predicate run against the current `SessionInfo`.
    pub allowed:  Callback<SessionInfo, bool>,
}

#[function_component(RoleGuard)]
pub fn role_guard(props: &RoleGuardProps) -> Html {
    let auth = use_context::<AuthContext>().expect("AuthContext missing");

    if auth.loading {
        return html! { <div class="loading-screen">{ "Loading…" }</div> };
    }

    if !auth.is_authenticated() {
        return html! { <Redirect<Route> to={Route::Login} /> };
    }

    let session = auth.session.as_ref().cloned().unwrap_or_default();
    if props.allowed.emit(session) {
        html! { { for props.children.iter() } }
    } else {
        html! {
            <div class="error-page error-page--403">
                <h1>{ "403 — Access Denied" }</h1>
                <p>{ "Your role does not have permission to access this area." }</p>
                <Link<Route> to={Route::Inbox}>{ "Return to inbox" }</Link<Route>>
            </div>
        }
    }
}
