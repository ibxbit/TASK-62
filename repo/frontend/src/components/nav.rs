/// Sidebar navigation with role-aware link visibility.
///
/// Each nav item is shown or hidden based on the current user's role.
/// Active route is highlighted via CSS class matching.
/// Sign-out dispatches `AuthAction::Logout`, calls the logout API (best-effort),
/// then navigates to `/login`.
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::{
    pages::Route,
    services::auth_service,
    store::auth_store::{AuthAction, AuthContext},
};

#[function_component(NavSidebar)]
pub fn nav_sidebar() -> Html {
    let auth      = use_context::<AuthContext>().expect("AuthContext missing");
    let navigator = use_navigator();
    let session   = auth.session.as_ref();

    let role = session.map(|s| s.role.as_str()).unwrap_or("");

    // Role capability flags
    let is_admin      = role == "operations_admin";
    let is_dispatcher = role == "dispatcher";
    let is_finance    = role == "finance_analyst";
    let can_ops       = matches!(role, "operations_admin" | "dispatcher");
    let can_finance   = matches!(role, "operations_admin" | "finance_analyst");
    let can_reporting = true; // all authenticated roles
    let can_alerts    = matches!(role, "operations_admin" | "dispatcher" | "finance_analyst");

    // ── Sign-out: dispatch Logout, fire-and-forget server call, navigate ─────
    let on_logout = {
        let auth = auth.clone();
        let nav  = navigator.clone();
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            auth.dispatch(AuthAction::Logout);
            spawn_local(async { let _ = auth_service::logout().await; });
            if let Some(n) = nav.as_ref() {
                n.push(&Route::Login);
            }
        })
    };

    html! {
        <nav class="sidebar">
            <div class="sidebar__brand">
                <span class="sidebar__brand-text">{ "TransitOps" }</span>
            </div>

            <ul class="sidebar__nav">

                // ── Ops Config (Admin + Dispatcher) ───────────────────────
                if can_ops {
                    <li class="sidebar__section-label">{ "Operations" }</li>
                    <li>
                        <Link<Route> to={Route::ConfigList}>
                            { "Config Versions" }
                        </Link<Route>>
                    </li>
                    <li>
                        <Link<Route> to={Route::ConfigDiff}>
                            { "Config Diff" }
                        </Link<Route>>
                    </li>
                    if is_admin {
                        <li>
                            <Link<Route> to={Route::RolloutManager}>
                                { "Rollout Manager" }
                            </Link<Route>>
                        </li>
                        <li>
                            <Link<Route> to={Route::OpsRoutes}>
                                { "Route Management" }
                            </Link<Route>>
                        </li>
                        <li>
                            <Link<Route> to={Route::OpsStops}>
                                { "Stop Management" }
                            </Link<Route>>
                        </li>
                        <li>
                            <Link<Route> to={Route::OpsCalendars}>
                                { "Trip Calendars" }
                            </Link<Route>>
                        </li>
                        <li>
                            <Link<Route> to={Route::OpsFareRules}>
                                { "Fare Rules" }
                            </Link<Route>>
                        </li>
                        <li>
                            <Link<Route> to={Route::OpsChangeRefundRules}>
                                { "Change & Refund Rules" }
                            </Link<Route>>
                        </li>
                    }
                }

                // ── Dispatcher ────────────────────────────────────────────
                if is_dispatcher || is_admin {
                    <li class="sidebar__section-label">{ "Dispatch" }</li>
                    <li>
                        <Link<Route> to={Route::Trips}>
                            { "Trip Adjustments" }
                        </Link<Route>>
                    </li>
                    <li>
                        <Link<Route> to={Route::Conflicts}>
                            { "Conflict Monitor" }
                        </Link<Route>>
                    </li>
                }

                // ── Finance ───────────────────────────────────────────────
                if can_finance {
                    <li class="sidebar__section-label">{ "Finance" }</li>
                    <li>
                        <Link<Route> to={Route::Statements}>
                            { "Statement Import" }
                        </Link<Route>>
                    </li>
                    <li>
                        <Link<Route> to={Route::Reconciliation}>
                            { "Reconciliation" }
                        </Link<Route>>
                    </li>
                    <li>
                        <Link<Route> to={Route::Refunds}>
                            { "Refunds" }
                        </Link<Route>>
                    </li>
                }

                // ── Notifications (all roles) ─────────────────────────────
                <li class="sidebar__section-label">{ "Notifications" }</li>
                <li>
                    <Link<Route> to={Route::Inbox}>
                        { "Inbox" }
                    </Link<Route>>
                </li>
                <li>
                    <Link<Route> to={Route::Subscriptions}>
                        { "Subscriptions" }
                    </Link<Route>>
                </li>
                <li>
                    <Link<Route> to={Route::Preferences}>
                        { "Preferences & DND" }
                    </Link<Route>>
                </li>

                // ── Reporting ─────────────────────────────────────────────
                if can_reporting {
                    <li class="sidebar__section-label">{ "Reporting" }</li>
                    <li>
                        <Link<Route> to={Route::Metrics}>
                            { "KPI Metrics" }
                        </Link<Route>>
                    </li>
                    <li>
                        <Link<Route> to={Route::ReportSchedules}>
                            { "Report Schedules" }
                        </Link<Route>>
                    </li>
                    <li>
                        <Link<Route> to={Route::ReportRuns}>
                            { "Report Runs" }
                        </Link<Route>>
                    </li>
                }

                // ── Alerting ──────────────────────────────────────────────
                if can_alerts {
                    <li class="sidebar__section-label">{ "Alerting" }</li>
                    <li>
                        <Link<Route> to={Route::Alerts}>
                            { "Alert Dashboard" }
                        </Link<Route>>
                    </li>
                    if is_admin {
                        <li>
                            <Link<Route> to={Route::AlertRules}>
                                { "Alert Rules" }
                            </Link<Route>>
                        </li>
                    }
                }

            </ul>

            // ── User info + logout ─────────────────────────────────────────
            <div class="sidebar__footer">
                if let Some(s) = session {
                    <span class="sidebar__username">{ &s.username }</span>
                    <span class="sidebar__role">{ &s.role }</span>
                }
                <button onclick={on_logout} class="sidebar__logout btn--link">
                    { "Sign out" }
                </button>
            </div>
        </nav>
    }
}
