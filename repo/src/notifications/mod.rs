/// Notification delivery pipeline with subscription rule engine.
///
/// Modules
/// ───────
///   bus      — background fan-out, DND queuing, queue flush; spawn at startup
///   rules    — rule evaluation: keyword, topic, entity_threshold, spike
///   models   — shared types (request / response / DB rows)
///   handlers — Actix-web route handlers
///
/// Route tree  (all under `/notifications`)
/// ─────────────────────────────────────────────────────────────────────────────
///
///   Inbox
///     GET    /notifications                      list delivered/queued/read
///     GET    /notifications/unread-count         badge counts (unread + queued)
///     POST   /notifications/read-all             bulk mark-read
///     POST   /notifications/{id}/read            mark single delivery read
///     POST   /notifications/{id}/dismiss         dismiss delivery
///
///   DND Preferences
///     GET    /notifications/preferences          current DND window
///     PUT    /notifications/preferences          update DND window
///
///   Event-type Subscriptions
///     GET    /notifications/subscriptions        all types + user's opt-ins
///     PUT    /notifications/subscriptions        bulk-replace opt-ins
///
///   Subscription Rules (keyword | topic | entity_threshold | spike)
///     GET    /notifications/rules                my rules
///     POST   /notifications/rules                create rule
///     GET    /notifications/rules/{id}           get one rule
///     PUT    /notifications/rules/{id}           update rule
///     DELETE /notifications/rules/{id}           delete rule
///     POST   /notifications/rules/{id}/toggle    enable / disable
///
///   Announcements
///     POST   /notifications/announce             broadcast (ops_admin only)
///
///   Channel Preferences
///     GET    /notifications/channels             list user's channel opt-ins
///     PUT    /notifications/channels/{channel}   upsert address + enable flag
///     DELETE /notifications/channels/{channel}   remove channel preference
///
/// Registration order: all static paths are registered before `/{id}` paths
/// to prevent Actix-web route shadowing.
pub mod adapters;
pub mod bus;
pub mod handlers;
pub mod models;
pub mod rules;

use actix_web::web;

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/notifications")
            // ── Inbox (static) ───────────────────────────────────────────────
            .service(
                web::resource("")
                    .route(web::get().to(handlers::list_deliveries)),
            )
            .service(
                web::resource("/unread-count")
                    .route(web::get().to(handlers::unread_count)),
            )
            .service(
                web::resource("/read-all")
                    .route(web::post().to(handlers::read_all)),
            )
            // ── DND preferences ──────────────────────────────────────────────
            .service(
                web::resource("/preferences")
                    .route(web::get().to(handlers::get_preferences))
                    .route(web::put().to(handlers::update_preferences)),
            )
            // ── Event-type subscriptions ─────────────────────────────────────
            .service(
                web::resource("/subscriptions")
                    .route(web::get().to(handlers::list_subscriptions))
                    .route(web::put().to(handlers::update_subscriptions)),
            )
            // ── Subscription rules (collection) ──────────────────────────────
            .service(
                web::resource("/rules")
                    .route(web::get().to(handlers::list_rules))
                    .route(web::post().to(handlers::create_rule)),
            )
            // ── Announcements ─────────────────────────────────────────────────
            .service(
                web::resource("/announce")
                    .route(web::post().to(handlers::announce)),
            )
            // ── Delivery receipts ────────────────────────────────────────────
            // Must be registered before /{id} so "receipt" is not matched as a UUID.
            .service(
                web::resource("/receipt")
                    .route(web::post().to(handlers::receipt)),
            )
            // ── Channel preferences (static, before /{id}) ───────────────────
            .service(
                web::resource("/channels")
                    .route(web::get().to(handlers::list_channel_prefs)),
            )
            .service(
                web::resource("/channels/{channel}")
                    .route(web::put().to(handlers::upsert_channel_pref))
                    .route(web::delete().to(handlers::delete_channel_pref)),
            )
            // ── Parameterised: rules/{id} (before /{id} to avoid shadowing) ──
            .service(
                web::resource("/rules/{id}")
                    .route(web::get().to(handlers::get_rule))
                    .route(web::put().to(handlers::update_rule))
                    .route(web::delete().to(handlers::delete_rule)),
            )
            .service(
                web::resource("/rules/{id}/toggle")
                    .route(web::post().to(handlers::toggle_rule)),
            )
            // ── Parameterised: inbox /{id} ────────────────────────────────────
            // GET /{id} registered before /{id}/read and /{id}/dismiss so that
            // Actix matches the sub-paths correctly via the radix tree.
            .service(
                web::resource("/{id}")
                    .route(web::get().to(handlers::get_notification)),
            )
            .service(
                web::resource("/{id}/read")
                    .route(web::post().to(handlers::mark_read)),
            )
            .service(
                web::resource("/{id}/dismiss")
                    .route(web::post().to(handlers::dismiss)),
            ),
    );
}
