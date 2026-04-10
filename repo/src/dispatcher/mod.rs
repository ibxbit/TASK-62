pub mod conflicts;
pub mod events;
pub mod handlers;
pub mod models;

use actix_web::web;

/// Register all dispatcher routes under `/dispatcher`.
///
/// Route tree:
///
///   Trip lifecycle
///     PATCH  /dispatcher/trips/{id}
///     POST   /dispatcher/trips/{id}/assign
///     POST   /dispatcher/trips/{id}/start
///     POST   /dispatcher/trips/{id}/complete
///     POST   /dispatcher/trips/{id}/cancel
///
///   Conflict management
///     GET    /dispatcher/trips/{id}/conflicts
///     POST   /dispatcher/trips/{id}/check
///     GET    /dispatcher/conflicts              ?severity=
///     POST   /dispatcher/conflicts/{id}/acknowledge
///     POST   /dispatcher/conflicts/{id}/resolve
///
///   Monitoring
///     GET    /dispatcher/monitor/dashboard
///     GET    /dispatcher/monitor/upcoming       ?window_minutes=
///     GET    /dispatcher/monitor/active
///     GET    /dispatcher/monitor/unassigned
///     POST   /dispatcher/monitor/check-approaching
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/dispatcher")
            // ── Trip lifecycle ──────────────────────────────────────────────
            .service(
                web::scope("/trips")
                    .service(
                        web::resource("/{id}")
                            .route(web::patch().to(handlers::patch_trip)),
                    )
                    .service(
                        web::resource("/{id}/assign")
                            .route(web::post().to(handlers::assign_driver)),
                    )
                    .service(
                        web::resource("/{id}/start")
                            .route(web::post().to(handlers::start_trip)),
                    )
                    .service(
                        web::resource("/{id}/complete")
                            .route(web::post().to(handlers::complete_trip)),
                    )
                    .service(
                        web::resource("/{id}/cancel")
                            .route(web::post().to(handlers::cancel_trip)),
                    )
                    .service(
                        web::resource("/{id}/conflicts")
                            .route(web::get().to(handlers::get_trip_conflicts)),
                    )
                    .service(
                        web::resource("/{id}/check")
                            .route(web::post().to(handlers::check_trip_conflicts)),
                    ),
            )
            // ── Conflict management ─────────────────────────────────────────
            .service(
                web::scope("/conflicts")
                    .service(
                        web::resource("")
                            .route(web::get().to(handlers::list_conflicts)),
                    )
                    .service(
                        web::resource("/{id}/acknowledge")
                            .route(web::post().to(handlers::acknowledge_conflict)),
                    )
                    .service(
                        web::resource("/{id}/resolve")
                            .route(web::post().to(handlers::resolve_conflict)),
                    ),
            )
            // ── Monitoring ──────────────────────────────────────────────────
            .service(
                web::scope("/monitor")
                    .service(
                        web::resource("/dashboard")
                            .route(web::get().to(handlers::dashboard)),
                    )
                    .service(
                        web::resource("/upcoming")
                            .route(web::get().to(handlers::upcoming_trips)),
                    )
                    .service(
                        web::resource("/active")
                            .route(web::get().to(handlers::active_trips)),
                    )
                    .service(
                        web::resource("/unassigned")
                            .route(web::get().to(handlers::unassigned_trips)),
                    )
                    .service(
                        web::resource("/check-approaching")
                            .route(web::post().to(handlers::check_approaching)),
                    ),
            ),
    );
}
