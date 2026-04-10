/// Application route definitions and top-level router switch.
///
/// `Route` is the single source of truth for all navigable paths in the
/// backoffice SPA.  Each variant maps to one page component; role-gated
/// variants are wrapped in `RoleGuard` inside `switch()`.
pub mod alerting;
pub mod dispatcher;
pub mod finance;
pub mod notifications;
pub mod ops;
pub mod reporting;

mod login;
pub use login::LoginPage;

use yew::prelude::*;
use yew_router::prelude::*;

use crate::{
    components::role_guard::{AuthGuard, RoleGuard},
    types::auth::SessionInfo,
};

use alerting::{alert_rules::AlertRulesPage, alerts::AlertsPage};
use dispatcher::{conflicts::ConflictsPage, trips::TripsPage};
use finance::{
    reconciliation::ReconciliationPage,
    refunds::RefundsPage,
    statements::StatementsPage,
};
use notifications::{
    inbox::InboxPage,
    preferences::PreferencesPage,
    subscriptions::SubscriptionsPage,
};
use ops::{
    calendars_admin::CalendarsAdminPage,
    change_refund_rules::ChangeRefundRulesPage,
    config_diff::ConfigDiffPage,
    config_list::ConfigListPage,
    fare_rules::FareRulesPage,
    rollout::RolloutManagerPage,
    routes_admin::RoutesAdminPage,
    stops_admin::StopsAdminPage,
};
use reporting::{metrics::MetricsPage, runs::ReportRunsPage, schedules::SchedulesPage};

// ── Route enum ────────────────────────────────────────────────────────────────

#[derive(Clone, Routable, PartialEq)]
pub enum Route {
    // Auth
    #[at("/login")]
    Login,

    // Operations (config lifecycle)
    #[at("/ops/config")]
    ConfigList,
    #[at("/ops/diff")]
    ConfigDiff,
    #[at("/ops/rollout")]
    RolloutManager,

    // Operations admin (entity management)
    #[at("/ops/routes")]
    OpsRoutes,
    #[at("/ops/stops")]
    OpsStops,
    #[at("/ops/calendars")]
    OpsCalendars,
    #[at("/ops/fare-rules")]
    OpsFareRules,
    #[at("/ops/change-refund-rules")]
    OpsChangeRefundRules,

    // Dispatcher
    #[at("/dispatch/trips")]
    Trips,
    #[at("/dispatch/conflicts")]
    Conflicts,

    // Finance
    #[at("/finance/statements")]
    Statements,
    #[at("/finance/reconciliation")]
    Reconciliation,
    #[at("/finance/refunds")]
    Refunds,

    // Notifications (all roles)
    #[at("/notifications")]
    Inbox,
    #[at("/notifications/subscriptions")]
    Subscriptions,
    #[at("/notifications/preferences")]
    Preferences,

    // Reporting
    #[at("/reporting/metrics")]
    Metrics,
    #[at("/reporting/schedules")]
    ReportSchedules,
    #[at("/reporting/runs")]
    ReportRuns,

    // Alerting
    #[at("/alerts")]
    Alerts,
    #[at("/alerts/rules")]
    AlertRules,

    // Default redirect
    #[not_found]
    #[at("/404")]
    NotFound,
}

// ── Switch function ───────────────────────────────────────────────────────────

pub fn switch(route: Route) -> Html {
    match route {
        Route::Login => html! { <LoginPage /> },

        // ── Ops: Admin + Dispatcher (config lifecycle) ────────────────────────
        Route::ConfigList => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin() || s.is_dispatcher())}>
                <ConfigListPage />
            </RoleGuard>
        },
        Route::ConfigDiff => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin() || s.is_dispatcher())}>
                <ConfigDiffPage />
            </RoleGuard>
        },
        Route::RolloutManager => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin())}>
                <RolloutManagerPage />
            </RoleGuard>
        },

        // ── Ops: Admin only (entity management) ──────────────────────────────
        Route::OpsRoutes => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin())}>
                <RoutesAdminPage />
            </RoleGuard>
        },
        Route::OpsStops => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin())}>
                <StopsAdminPage />
            </RoleGuard>
        },
        Route::OpsCalendars => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin())}>
                <CalendarsAdminPage />
            </RoleGuard>
        },
        Route::OpsFareRules => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin())}>
                <FareRulesPage />
            </RoleGuard>
        },
        Route::OpsChangeRefundRules => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin())}>
                <ChangeRefundRulesPage />
            </RoleGuard>
        },

        // ── Dispatcher ────────────────────────────────────────────────────────
        Route::Trips => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin() || s.is_dispatcher())}>
                <TripsPage />
            </RoleGuard>
        },
        Route::Conflicts => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin() || s.is_dispatcher())}>
                <ConflictsPage />
            </RoleGuard>
        },

        // ── Finance ───────────────────────────────────────────────────────────
        Route::Statements => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_finance() || s.is_admin())}>
                <StatementsPage />
            </RoleGuard>
        },
        Route::Reconciliation => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_finance() || s.is_admin())}>
                <ReconciliationPage />
            </RoleGuard>
        },
        Route::Refunds => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_finance() || s.is_admin())}>
                <RefundsPage />
            </RoleGuard>
        },

        // ── Notifications — all authenticated users ───────────────────────────
        Route::Inbox => html! {
            <AuthGuard><InboxPage /></AuthGuard>
        },
        Route::Subscriptions => html! {
            <AuthGuard><SubscriptionsPage /></AuthGuard>
        },
        Route::Preferences => html! {
            <AuthGuard><PreferencesPage /></AuthGuard>
        },

        // ── Reporting — all authenticated users ───────────────────────────────
        Route::Metrics => html! {
            <AuthGuard><MetricsPage /></AuthGuard>
        },
        Route::ReportSchedules => html! {
            <AuthGuard><SchedulesPage /></AuthGuard>
        },
        Route::ReportRuns => html! {
            <AuthGuard><ReportRunsPage /></AuthGuard>
        },

        // ── Alerting ──────────────────────────────────────────────────────────
        Route::Alerts => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| {
                s.is_admin() || s.is_dispatcher() || s.is_finance()
            })}>
                <AlertsPage />
            </RoleGuard>
        },
        Route::AlertRules => html! {
            <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin())}>
                <AlertRulesPage />
            </RoleGuard>
        },

        Route::NotFound => html! {
            <div class="error-page error-page--404">
                <h1>{ "404 — Page Not Found" }</h1>
                <a href="/notifications">{ "Go to inbox" }</a>
            </div>
        },
    }
}
