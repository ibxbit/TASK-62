/// Immutable audit logging module.
///
/// Provides append-only audit log writes (`writer`) and read-only query
/// endpoints for auditors (`handlers`).  The underlying `audit.audit_logs`
/// table is partitioned by year and protected at the database level — the
/// application role holds only SELECT + INSERT, never UPDATE or DELETE.
///
/// ## Route tree  (`/audit`)
///
/// ```text
/// GET /audit/logs         list_logs   (filters: actor_id, domain, entity_type,
///                                               entity_id, action, date_from/to)
/// GET /audit/logs/{id}    get_log
/// ```
///
/// ## Retention & Immutability
///
/// Each row has a generated `retention_until` column (created_at + 7 years).
/// Expired entries are visible via the `audit.expired_logs` view (migration 010)
/// and can be purged by a scheduled job.
///
/// Immutability: The audit.audit_logs table is append-only. The DB role used by the app
/// has only SELECT and INSERT privileges (no UPDATE/DELETE). This is enforced in migrations.
///
/// Retention enforcement: See db/migrations/010_audit_extensions.sql for the expired_logs view
/// and purge guardrail. Purge jobs must use this view to avoid deleting in-window entries.
pub mod handlers;
pub mod writer;

use actix_web::web;
use handlers::{get_log, list_logs};

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/audit")
            .service(
                web::resource("/logs")
                    .route(web::get().to(list_logs)),
            )
            .service(
                web::resource("/logs/{id}")
                    .route(web::get().to(get_log)),
            ),
    );
}
