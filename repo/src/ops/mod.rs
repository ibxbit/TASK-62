pub mod config;
pub mod diff;
pub mod models;
pub mod routes;
pub mod trips;

use actix_web::web;

/// Register all `/ops/*` routes.
///
/// Route tree:
///   /ops/routes                   GET, POST
///   /ops/routes/{id}              GET, PUT, DELETE
///   /ops/routes/{id}/publish      POST
///   /ops/routes/{id}/unpublish    POST
///   /ops/routes/{id}/schedule     POST
///   /ops/routes/{id}/stops        GET, POST
///   /ops/routes/{id}/stops/{sid}  GET, PUT, DELETE
///
///   /ops/trips                    GET, POST
///   /ops/trips/{id}               GET, PUT, DELETE
///   /ops/trips/{id}/publish       POST
///   /ops/trips/{id}/unpublish     POST
///   /ops/trips/{id}/schedule      POST
///
///   /ops/calendars                GET, POST
///   /ops/calendars/{id}           GET, PUT, DELETE
///
///   /ops/configs/{tid}/versions          GET, POST
///   /ops/configs/{tid}/versions/{vid}    GET, PUT
///   /ops/configs/{tid}/versions/diff     GET  (?v1=…&v2=…)
///   /ops/configs/{tid}/versions/{vid}/publish     POST
///   /ops/configs/{tid}/versions/{vid}/unpublish   POST
///   /ops/configs/{tid}/versions/{vid}/schedule    POST
///   /ops/configs/{tid}/versions/{vid}/rollout     POST
///   /ops/configs/{tid}/rollout/{pid}              GET
///   /ops/configs/{tid}/rollout/{pid}/stages/{sid}/activate  POST
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/ops")
            // ── Routes ─────────────────────────────────────────────
            .route("/routes",                         web::get() .to(routes::list_routes))
            .route("/routes",                         web::post().to(routes::create_route))
            .route("/routes/{id}",                    web::get() .to(routes::get_route))
            .route("/routes/{id}",                    web::put() .to(routes::update_route))
            .route("/routes/{id}",                    web::delete().to(routes::delete_route))
            .route("/routes/{id}/publish",            web::post().to(routes::publish_route))
            .route("/routes/{id}/unpublish",          web::post().to(routes::unpublish_route))
            .route("/routes/{id}/schedule",           web::post().to(routes::schedule_route))
            // ── Stops (nested) ─────────────────────────────────────
            .route("/routes/{route_id}/stops",        web::get() .to(routes::list_stops))
            .route("/routes/{route_id}/stops",        web::post().to(routes::create_stop))
            .route("/routes/{route_id}/stops/{stop_id}", web::get()   .to(routes::get_stop))
            .route("/routes/{route_id}/stops/{stop_id}", web::put()   .to(routes::update_stop))
            .route("/routes/{route_id}/stops/{stop_id}", web::delete().to(routes::delete_stop))
            // ── Trips ──────────────────────────────────────────────
            .route("/trips",                          web::get() .to(trips::list_trips))
            .route("/trips",                          web::post().to(trips::create_trip))
            .route("/trips/{id}",                     web::get() .to(trips::get_trip))
            .route("/trips/{id}",                     web::put() .to(trips::update_trip))
            .route("/trips/{id}",                     web::delete().to(trips::delete_trip))
            .route("/trips/{id}/publish",             web::post().to(trips::publish_trip))
            .route("/trips/{id}/unpublish",           web::post().to(trips::unpublish_trip))
            .route("/trips/{id}/schedule",            web::post().to(trips::schedule_trip))
            // ── Calendars ──────────────────────────────────────────
            .route("/calendars",                      web::get() .to(trips::list_calendars))
            .route("/calendars",                      web::post().to(trips::create_calendar))
            .route("/calendars/{id}",                 web::get() .to(trips::get_calendar))
            .route("/calendars/{id}",                 web::put() .to(trips::update_calendar))
            .route("/calendars/{id}",                 web::delete().to(trips::delete_calendar))
            // ── Config versions ────────────────────────────────────
            // NOTE: /diff must be registered before /{vid} so it is not swallowed
            .route("/configs/{tid}/versions/diff",    web::get() .to(config::diff_config_versions))
            .route("/configs/{tid}/versions",         web::get() .to(config::list_versions))
            .route("/configs/{tid}/versions",         web::post().to(config::create_version))
            .route("/configs/{tid}/versions/{vid}",   web::get() .to(config::get_version))
            .route("/configs/{tid}/versions/{vid}",   web::put() .to(config::update_version))
            .route("/configs/{tid}/versions/{vid}/publish",   web::post().to(config::publish_version))
            .route("/configs/{tid}/versions/{vid}/unpublish", web::post().to(config::unpublish_version))
            .route("/configs/{tid}/versions/{vid}/schedule",  web::post().to(config::schedule_version))
            .route("/configs/{tid}/versions/{vid}/rollout",   web::post().to(config::create_rollout))
            // ── Rollout plan read + stage activation ───────────────
            .route("/configs/{tid}/rollout/{pid}",            web::get() .to(config::get_rollout_plan))
            .route(
                "/configs/{tid}/rollout/{pid}/stages/{sid}/activate",
                web::post().to(config::activate_rollout_stage),
            ),
    );
}
