/// Write-only audit log abstraction.
///
/// All writes go through `append`, which performs a plain INSERT with no
/// UPDATE or DELETE paths.  Database-level immutability is enforced by the
/// `GRANT SELECT, INSERT` grant on `audit.audit_logs` — the application role
/// cannot modify or delete rows.
///
/// Callers must treat audit failures as non-fatal:
/// ```rust
/// crate::audit::writer::log(&pool, &session, entry)
///     .await
///     .unwrap_or_else(|e| tracing::warn!(error = %e, "audit log failed"));
/// ```
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::middleware::AuthSession;

// ============================================================
// Action name constants
// ============================================================
pub const ACTION_LOGIN:            &str = "LOGIN";
pub const ACTION_LOGOUT:           &str = "LOGOUT";
pub const ACTION_CONFIG_PUBLISH:   &str = "CONFIG_PUBLISH";
pub const ACTION_CONFIG_UNPUBLISH: &str = "CONFIG_UNPUBLISH";
pub const ACTION_RECON_RUN:        &str = "RECONCILIATION_RUN";
pub const ACTION_EXPORT:           &str = "EXPORT";
pub const ACTION_METRIC_CREATE:    &str = "METRIC_CREATE";
pub const ACTION_METRIC_UPDATE:    &str = "METRIC_UPDATE";
pub const ACTION_METRIC_DELETE:    &str = "METRIC_DELETE";

// ============================================================
// Entry description
// ============================================================

/// Describes a single auditable event for an authenticated request.
pub struct AuditEntry<'a> {
    pub action:       &'a str,
    pub domain:       &'a str,
    pub entity_type:  &'a str,
    pub entity_id:    String,
    pub before_state: Option<serde_json::Value>,
    pub after_state:  Option<serde_json::Value>,
    pub metadata:     serde_json::Value,
}

// ============================================================
// Public — authenticated requests (AuthSession available)
// ============================================================

/// Write an immutable audit entry for an authenticated HTTP request.
pub async fn log(
    pool:    &PgPool,
    session: &AuthSession,
    entry:   AuditEntry<'_>,
) -> Result<(), sqlx::Error> {
    append(
        pool,
        Some(session.user_id),
        &session.username,
        &session.role,
        Some(session.session_id),
        entry.action,
        entry.domain,
        entry.entity_type,
        &entry.entity_id,
        entry.before_state,
        entry.after_state,
        None,   // ip_address not available from AuthSession
        None,   // user_agent not available from AuthSession
        entry.metadata,
    )
    .await
}

// ============================================================
// Public — login event (no AuthSession yet)
// ============================================================

/// Write an audit entry for a login event.
///
/// Called after the session row is committed, so `session_id` is known.
#[allow(clippy::too_many_arguments)]
pub async fn log_auth(
    pool:       &PgPool,
    user_id:    Uuid,
    username:   &str,
    role:       &str,
    session_id: Uuid,
    action:     &str,
    ip_address: Option<String>,
    metadata:   serde_json::Value,
) -> Result<(), sqlx::Error> {
    append(
        pool,
        Some(user_id),
        username,
        role,
        Some(session_id),
        action,
        "auth",
        "session",
        &session_id.to_string(),
        None,
        None,
        ip_address.as_deref(),
        None,
        metadata,
    )
    .await
}

// ============================================================
// Private — INSERT-only core
// ============================================================

#[allow(clippy::too_many_arguments)]
async fn append(
    pool:        &PgPool,
    actor_id:    Option<Uuid>,
    actor_user:  &str,
    actor_role:  &str,
    session_id:  Option<Uuid>,
    action:      &str,
    domain:      &str,
    entity_type: &str,
    entity_id:   &str,
    before:      Option<serde_json::Value>,
    after:       Option<serde_json::Value>,
    ip_address:  Option<&str>,
    user_agent:  Option<&str>,
    metadata:    serde_json::Value,
) -> Result<(), sqlx::Error> {
    // Use query() (non-macro) so the INET cast compiles without the
    // ipnetwork sqlx feature.  No UPDATE or DELETE path exists here.
    sqlx::query(
        r#"
        INSERT INTO audit.audit_logs
            (actor_id, actor_username, actor_role, session_id,
             action, domain, entity_type, entity_id,
             before_state, after_state, ip_address, user_agent, metadata)
        VALUES
            ($1, $2, $3, $4,
             $5, $6, $7, $8,
             $9, $10, $11::inet, $12, $13)
        "#,
    )
    .bind(actor_id)
    .bind(actor_user)
    .bind(actor_role)
    .bind(session_id)
    .bind(action)
    .bind(domain)
    .bind(entity_type)
    .bind(entity_id)
    .bind(before)
    .bind(after)
    .bind(ip_address)
    .bind(user_agent)
    .bind(metadata)
    .execute(pool)
    .await?;

    Ok(())
}
