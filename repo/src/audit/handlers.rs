use actix_web::{web, HttpResponse};
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::auth::middleware::AuthSession;
use crate::error::AppError;
use crate::rbac::permissions::Permission;
use crate::AppState;

// ============================================================
// Query params
// ============================================================

#[derive(serde::Deserialize)]
pub struct ListLogsQuery {
    pub actor_id:    Option<Uuid>,
    pub domain:      Option<String>,
    pub entity_type: Option<String>,
    pub entity_id:   Option<String>,
    pub action:      Option<String>,
    pub date_from:   Option<DateTime<Utc>>,
    pub date_to:     Option<DateTime<Utc>>,
    pub limit:       Option<i64>,
    pub offset:      Option<i64>,
}

// ============================================================
// DB row  (ip_address decoded as text via SQL cast)
// ============================================================

#[derive(sqlx::FromRow, serde::Serialize)]
struct AuditLogRow {
    id:              Uuid,
    actor_id:        Option<Uuid>,
    actor_username:  String,
    actor_role:      String,
    session_id:      Option<Uuid>,
    action:          String,
    domain:          String,
    entity_type:     String,
    entity_id:       String,
    before_state:    Option<serde_json::Value>,
    after_state:     Option<serde_json::Value>,
    ip_address:      Option<String>,
    user_agent:      Option<String>,
    metadata:        serde_json::Value,
    created_at:      DateTime<Utc>,
    retention_until: NaiveDate,
}

// ============================================================
// GET /audit/logs
// ============================================================

/// List audit log entries with optional filters.
///
/// Query params: actor_id, domain, entity_type, entity_id, action,
///               date_from, date_to, limit (1–200, default 50), offset
pub async fn list_logs(
    state:   web::Data<AppState>,
    session: AuthSession,
    query:   web::Query<ListLogsQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::AuditRead)?;

    let limit  = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);

    let rows = sqlx::query_as::<_, AuditLogRow>(
        r#"
        SELECT id, actor_id, actor_username, actor_role, session_id,
               action, domain, entity_type, entity_id,
               before_state, after_state,
               ip_address::text AS ip_address,
               user_agent, metadata,
               created_at, retention_until
        FROM   audit.audit_logs
        WHERE  ($1::uuid        IS NULL OR actor_id    = $1)
          AND  ($2::text        IS NULL OR domain      = $2)
          AND  ($3::text        IS NULL OR entity_type = $3)
          AND  ($4::text        IS NULL OR entity_id   = $4)
          AND  ($5::text        IS NULL OR action      = $5)
          AND  ($6::timestamptz IS NULL OR created_at >= $6)
          AND  ($7::timestamptz IS NULL OR created_at <= $7)
        ORDER  BY created_at DESC
        LIMIT  $8 OFFSET $9
        "#,
    )
    .bind(query.actor_id)
    .bind(query.domain.as_deref())
    .bind(query.entity_type.as_deref())
    .bind(query.entity_id.as_deref())
    .bind(query.action.as_deref())
    .bind(query.date_from)
    .bind(query.date_to)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(rows))
}

// ============================================================
// GET /audit/logs/{id}
// ============================================================

pub async fn get_log(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::AuditRead)?;
    let id = *path;

    let row = sqlx::query_as::<_, AuditLogRow>(
        r#"
        SELECT id, actor_id, actor_username, actor_role, session_id,
               action, domain, entity_type, entity_id,
               before_state, after_state,
               ip_address::text AS ip_address,
               user_agent, metadata,
               created_at, retention_until
        FROM   audit.audit_logs
        WHERE  id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Audit log entry not found".to_string()))?;

    Ok(HttpResponse::Ok().json(row))
}
