/// TransitOps backend library crate.
///
/// Exposes all domain modules so that integration tests in `tests/` can import
/// pure functions and types without needing a running database.  The binary
/// entry-point (`src/main.rs`) uses these modules via `use transitops_backend::`.
pub mod alerting;
pub mod audit;
pub mod auth;
pub mod config;
pub mod crypto;
pub mod db;
pub mod dispatcher;
pub mod error;
pub mod notifications;
pub mod ops;
pub mod payments;
pub mod rbac;
pub mod reconciliation;
pub mod reporting;
pub mod scheduler;

/// Shared application state — cloned cheaply via `Arc` inside `web::Data`.
pub struct AppState {
    pub db:     sqlx::PgPool,
    pub config: config::Config,
    pub crypto: crypto::FieldEncryptor,
}
