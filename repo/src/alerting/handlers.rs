use actix_web::{web, HttpResponse};
use uuid::Uuid;

use crate::auth::middleware::AuthSession;
use crate::error::AppError;
use crate::rbac::permissions::Permission;
use crate::AppState;

use super::models::{
    AcknowledgeRequest, AlertResponse, AlertRow, CloseRequest, ListAlertsQuery,
};

// ============================================================
// GET /alerts
// ============================================================

/// List alerts with optional filters.
///
/// Query params:
///   - `status`     — open | acknowledged | closed
///   - `severity`   — info | warning | critical
///   - `alert_type` — reconciliation_mismatch | kpi_anomaly
///   - `limit`      — 1–200, default 50
///   - `offset`     — default 0
pub async fn list_alerts(
    state:   web::Data<AppState>,
    session: AuthSession,
    query:   web::Query<ListAlertsQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::AlertsRead)?;

    const VALID_STATUSES:    &[&str] = &["open", "acknowledged", "closed"];
    const VALID_SEVERITIES:  &[&str] = &["info", "warning", "critical"];
    const VALID_ALERT_TYPES: &[&str] = &["reconciliation_mismatch", "kpi_anomaly"];

    if let Some(s) = &query.status {
        if !VALID_STATUSES.contains(&s.as_str()) {
            return Err(AppError::BadRequest(format!(
                "Invalid status '{}'; must be one of: open, acknowledged, closed",
                s
            )));
        }
    }
    if let Some(s) = &query.severity {
        if !VALID_SEVERITIES.contains(&s.as_str()) {
            return Err(AppError::BadRequest(format!(
                "Invalid severity '{}'; must be one of: info, warning, critical",
                s
            )));
        }
    }
    if let Some(s) = &query.alert_type {
        if !VALID_ALERT_TYPES.contains(&s.as_str()) {
            return Err(AppError::BadRequest(format!(
                "Invalid alert_type '{}'; must be one of: reconciliation_mismatch, kpi_anomaly",
                s
            )));
        }
    }

    let limit  = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);

    let rows: Vec<AlertRow> = sqlx::query_as!(
        AlertRow,
        r#"
        SELECT id, alert_type, severity, status, source_domain, source_entity_id,
               title, description, payload,
               acknowledged_by, acknowledged_at,
               closed_by, closed_at, close_reason,
               created_at, updated_at
        FROM alerting.alerts
        WHERE ($1::text IS NULL OR status     = $1)
          AND ($2::text IS NULL OR severity   = $2)
          AND ($3::text IS NULL OR alert_type = $3)
        ORDER BY created_at DESC
        LIMIT $4 OFFSET $5
        "#,
        query.status.as_deref(),
        query.severity.as_deref(),
        query.alert_type.as_deref(),
        limit,
        offset,
    )
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<AlertResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

// ============================================================
// GET /alerts/stats
// ============================================================

/// Aggregated alert counts grouped by status and severity.
///
/// Useful for dashboard widgets (e.g. "3 critical open alerts").
pub async fn alert_stats(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::AlertsRead)?;

    #[derive(sqlx::FromRow)]
    struct StatsRow {
        status:   String,
        severity: String,
        cnt:      Option<i64>,
    }

    let rows = sqlx::query_as!(
        StatsRow,
        r#"
        SELECT status, severity, COUNT(*) AS cnt
        FROM   alerting.alerts
        GROUP  BY status, severity
        ORDER  BY status, severity
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    // Accumulate by_status and by_severity totals.
    let mut by_status:   std::collections::HashMap<String, i64> = Default::default();
    let mut by_severity: std::collections::HashMap<String, i64> = Default::default();

    for r in &rows {
        let n = r.cnt.unwrap_or(0);
        *by_status.entry(r.status.clone()).or_default()     += n;
        *by_severity.entry(r.severity.clone()).or_default() += n;
    }

    let open_total = *by_status.get("open").unwrap_or(&0);

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "open_total":  open_total,
        "by_status":   by_status,
        "by_severity": by_severity,
    })))
}

// ============================================================
// GET /alerts/{id}
// ============================================================

pub async fn get_alert(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::AlertsRead)?;
    let id = *path;

    let row: Option<AlertRow> = sqlx::query_as!(
        AlertRow,
        r#"
        SELECT id, alert_type, severity, status, source_domain, source_entity_id,
               title, description, payload,
               acknowledged_by, acknowledged_at,
               closed_by, closed_at, close_reason,
               created_at, updated_at
        FROM alerting.alerts
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Alert not found".to_string()))?;

    Ok(HttpResponse::Ok().json(AlertResponse::from(row)))
}

// ============================================================
// POST /alerts/{id}/acknowledge
// ============================================================

/// Transition an alert from `open` → `acknowledged`.
///
/// Only `open` alerts can be acknowledged.  The authenticated user is
/// recorded as `acknowledged_by`.  A notification event is fired so
/// that other subscribers see the workflow update.
pub async fn acknowledge_alert(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<AcknowledgeRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::AlertsManage)?;
    let id = *path;

    // Fetch current status.
    let existing = sqlx::query!(
        "SELECT status FROM alerting.alerts WHERE id = $1",
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Alert not found".to_string()))?;

    if existing.status != "open" {
        return Err(AppError::BadRequest(format!(
            "Alert cannot be acknowledged from status '{}' (must be open)",
            existing.status
        )));
    }

    let row = sqlx::query_as!(
        AlertRow,
        r#"
        UPDATE alerting.alerts
        SET    status          = 'acknowledged',
               acknowledged_by = $2,
               acknowledged_at = now(),
               updated_at      = now()
        WHERE  id = $1
        RETURNING id, alert_type, severity, status, source_domain, source_entity_id,
                  title, description, payload,
                  acknowledged_by, acknowledged_at,
                  closed_by, closed_at, close_reason,
                  created_at, updated_at
        "#,
        id,
        session.user_id,
    )
    .fetch_one(&state.db)
    .await?;

    // Fire workflow notification event.
    let notes_val = body
        .notes
        .as_deref()
        .map(|n| serde_json::Value::String(n.to_string()))
        .unwrap_or(serde_json::Value::Null);

    sqlx::query!(
        r#"
        INSERT INTO notifications.events
            (event_type, source_domain, source_entity_id, actor_id, payload)
        VALUES ('alerts.alert.acknowledged', 'alerting', $1, $2, $3)
        "#,
        id,
        session.user_id,
        serde_json::json!({
            "alert_id":        id,
            "acknowledged_by": session.user_id,
            "notes":           notes_val,
        }),
    )
    .execute(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(AlertResponse::from(row)))
}

// ============================================================
// POST /alerts/{id}/close
// ============================================================

/// Transition an alert from `open` or `acknowledged` → `closed`.
///
/// Alerts can be closed directly from `open` (skipping acknowledgement)
/// or after having been acknowledged.  Once closed, the partial-unique
/// index on `(alert_type, source_entity_id) WHERE status = 'open'` is
/// released, allowing a new alert to be raised for the same source in
/// the future.
pub async fn close_alert(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<CloseRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::AlertsManage)?;
    let id = *path;

    let existing = sqlx::query!(
        "SELECT status FROM alerting.alerts WHERE id = $1",
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Alert not found".to_string()))?;

    if existing.status == "closed" {
        return Err(AppError::BadRequest("Alert is already closed".to_string()));
    }

    let row = sqlx::query_as!(
        AlertRow,
        r#"
        UPDATE alerting.alerts
        SET    status       = 'closed',
               closed_by    = $2,
               closed_at    = now(),
               close_reason = $3,
               updated_at   = now()
        WHERE  id = $1
        RETURNING id, alert_type, severity, status, source_domain, source_entity_id,
                  title, description, payload,
                  acknowledged_by, acknowledged_at,
                  closed_by, closed_at, close_reason,
                  created_at, updated_at
        "#,
        id,
        session.user_id,
        body.reason.as_deref(),
    )
    .fetch_one(&state.db)
    .await?;

    // Fire workflow notification event.
    let reason_val = body
        .reason
        .as_deref()
        .map(|r| serde_json::Value::String(r.to_string()))
        .unwrap_or(serde_json::Value::Null);

    sqlx::query!(
        r#"
        INSERT INTO notifications.events
            (event_type, source_domain, source_entity_id, actor_id, payload)
        VALUES ('alerts.alert.closed', 'alerting', $1, $2, $3)
        "#,
        id,
        session.user_id,
        serde_json::json!({
            "alert_id":  id,
            "closed_by": session.user_id,
            "reason":    reason_val,
        }),
    )
    .execute(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(AlertResponse::from(row)))
}
