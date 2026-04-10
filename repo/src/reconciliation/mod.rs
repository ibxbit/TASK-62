pub mod discrepancy;
pub mod engine;
pub mod handlers;
pub mod importer;
pub mod models;

use actix_web::web;

use handlers::{
    list_statements, upload_statement,
    start_run, list_runs, get_run, list_items, run_summary,
};

/// Register all `/reconciliation` routes onto the Actix-web `ServiceConfig`.
///
/// Route tree:
///
/// ```text
/// POST  /reconciliation/statements              upload_statement
/// GET   /reconciliation/statements              list_statements
///
/// POST  /reconciliation/runs                    start_run
/// GET   /reconciliation/runs                    list_runs
/// GET   /reconciliation/runs/{id}               get_run
/// GET   /reconciliation/runs/{id}/summary       run_summary   ← STATIC before /items
/// GET   /reconciliation/runs/{id}/items         list_items
/// ```
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/reconciliation")
            // ---- statements ----
            .service(
                web::resource("/statements")
                    .route(web::get().to(list_statements))
                    .route(web::post().to(upload_statement)),
            )
            // ---- runs ----
            .service(
                web::resource("/runs")
                    .route(web::get().to(list_runs))
                    .route(web::post().to(start_run)),
            )
            // Static sub-paths registered before the bare /{id} resource
            .service(
                web::resource("/runs/{id}/summary")
                    .route(web::get().to(run_summary)),
            )
            .service(
                web::resource("/runs/{id}/items")
                    .route(web::get().to(list_items)),
            )
            .service(
                web::resource("/runs/{id}")
                    .route(web::get().to(get_run)),
            ),
    );
}
