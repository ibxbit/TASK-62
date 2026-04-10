/// Anomaly alerting module.
///
/// Provides lifecycle-tracked alerts sourced from reconciliation mismatches
/// and KPI deviations, with severity levels and an acknowledge / close workflow.
///
/// ## Detection sources
///
///   - **Reconciliation mismatches** — triggered synchronously by the reconciliation
///     engine after each run that contains discrepancies.
///   - **KPI anomalies** — evaluated asynchronously by a background task every
///     30 minutes; compares latest metric snapshot against a rolling baseline.
///
/// ## Alert lifecycle
///
/// ```text
///   open → acknowledged → closed
///   open →               closed
/// ```
///
/// ## Route tree  (`/alerts`)
///
/// ```text
/// GET  /alerts            list_alerts   (filters: status, severity, alert_type)
/// GET  /alerts/stats      alert_stats   (aggregated counts — dashboard widget)
/// GET  /alerts/{id}       get_alert
/// POST /alerts/{id}/acknowledge
/// POST /alerts/{id}/close
/// ```
pub mod detector;
pub mod handlers;
pub mod models;

use actix_web::web;

use handlers::{acknowledge_alert, alert_stats, close_alert, get_alert, list_alerts};

/// Register all `/alerts` routes onto the Actix-web `ServiceConfig`.
///
/// Static paths (`/stats`, `/{id}/acknowledge`, `/{id}/close`) are registered
/// before the bare `/{id}` resource to prevent Actix-web route shadowing.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/alerts")
            // ── Collection ────────────────────────────────────────────────────
            .service(
                web::resource("")
                    .route(web::get().to(list_alerts)),
            )
            // ── Aggregated stats (static, before /{id}) ───────────────────────
            .service(
                web::resource("/stats")
                    .route(web::get().to(alert_stats)),
            )
            // ── Workflow sub-paths (static, before /{id}) ─────────────────────
            .service(
                web::resource("/{id}/acknowledge")
                    .route(web::post().to(acknowledge_alert)),
            )
            .service(
                web::resource("/{id}/close")
                    .route(web::post().to(close_alert)),
            )
            // ── Single resource (after static sub-paths) ──────────────────────
            .service(
                web::resource("/{id}")
                    .route(web::get().to(get_alert)),
            ),
    );
}
