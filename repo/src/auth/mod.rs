pub mod handlers;
pub mod middleware;
pub mod models;
pub mod password;

use actix_web::web;

/// Register all `/auth/*` routes onto the app.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/auth")
            .route("/login",  web::post().to(handlers::login))
            .route("/logout", web::post().to(handlers::logout))
            .route("/session",web::get() .to(handlers::get_session))
            .route("/reauth", web::post().to(handlers::reauth)),
    );
}
