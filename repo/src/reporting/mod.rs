pub mod export;
pub mod handlers;
pub mod metrics;
pub mod models;
pub mod scheduler;

use actix_web::web;

use handlers::{
    // Metric definitions
    list_metrics, create_metric, get_metric, update_metric, delete_metric,
    // Compute
    compute_metrics,
    // Scheduled reports
    list_schedules, create_schedule, get_schedule, update_schedule, delete_schedule,
    trigger_run,
    // Runs
    list_runs, get_run,
    // Export
    export_run,
};

/// Register all `/reporting` routes onto the Actix-web `ServiceConfig`.
///
/// Route tree:
///
/// ```text
/// GET  /reporting/metrics                list_metrics
/// POST /reporting/metrics                create_metric
/// POST /reporting/metrics/compute        compute_metrics
/// GET  /reporting/metrics/{id}           get_metric
/// PUT  /reporting/metrics/{id}           update_metric
/// DELETE /reporting/metrics/{id}         delete_metric
///
/// GET  /reporting/schedules              list_schedules
/// POST /reporting/schedules              create_schedule
/// GET  /reporting/schedules/{id}         get_schedule
/// PUT  /reporting/schedules/{id}         update_schedule
/// DELETE /reporting/schedules/{id}       delete_schedule
/// POST /reporting/schedules/{id}/trigger trigger_run
///
/// GET  /reporting/runs                   list_runs
/// GET  /reporting/runs/{id}              get_run
/// GET  /reporting/runs/{id}/export       export_run
/// ```
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/reporting")
            // ---- metric definitions ----
            .service(
                web::resource("/metrics")
                    .route(web::get().to(list_metrics))
                    .route(web::post().to(create_metric)),
            )
            // Static /metrics/compute registered BEFORE parameterised /metrics/{id}
            .service(
                web::resource("/metrics/compute")
                    .route(web::post().to(compute_metrics)),
            )
            .service(
                web::resource("/metrics/{id}")
                    .route(web::get().to(get_metric))
                    .route(web::put().to(update_metric))
                    .route(web::delete().to(delete_metric)),
            )
            // ---- scheduled reports ----
            .service(
                web::resource("/schedules")
                    .route(web::get().to(list_schedules))
                    .route(web::post().to(create_schedule)),
            )
            // Static /schedules/{id}/trigger before parameterised /{id}
            .service(
                web::resource("/schedules/{id}/trigger")
                    .route(web::post().to(trigger_run)),
            )
            .service(
                web::resource("/schedules/{id}")
                    .route(web::get().to(get_schedule))
                    .route(web::put().to(update_schedule))
                    .route(web::delete().to(delete_schedule)),
            )
            // ---- runs ----
            .service(
                web::resource("/runs")
                    .route(web::get().to(list_runs)),
            )
            // Static /runs/{id}/export before parameterised /runs/{id}
            .service(
                web::resource("/runs/{id}/export")
                    .route(web::get().to(export_run)),
            )
            .service(
                web::resource("/runs/{id}")
                    .route(web::get().to(get_run)),
            ),
    );
}
