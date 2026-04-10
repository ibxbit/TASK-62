use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================
// DB row type
// ============================================================

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AlertRow {
    pub id:               Uuid,
    pub alert_type:       String,
    pub severity:         String,
    pub status:           String,
    pub source_domain:    String,
    pub source_entity_id: Option<Uuid>,
    pub title:            String,
    pub description:      Option<String>,
    pub payload:          serde_json::Value,
    pub acknowledged_by:  Option<Uuid>,
    pub acknowledged_at:  Option<DateTime<Utc>>,
    pub closed_by:        Option<Uuid>,
    pub closed_at:        Option<DateTime<Utc>>,
    pub close_reason:     Option<String>,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

// ============================================================
// API request types
// ============================================================

/// GET /alerts query params.
#[derive(Debug, Deserialize)]
pub struct ListAlertsQuery {
    /// Filter by status: open | acknowledged | closed
    pub status:     Option<String>,
    /// Filter by severity: info | warning | critical
    pub severity:   Option<String>,
    /// Filter by alert_type: reconciliation_mismatch | kpi_anomaly
    pub alert_type: Option<String>,
    pub limit:      Option<i64>,
    pub offset:     Option<i64>,
}

/// POST /alerts/{id}/acknowledge
#[derive(Debug, Deserialize)]
pub struct AcknowledgeRequest {
    /// Optional operator notes attached to the acknowledgement.
    pub notes: Option<String>,
}

/// POST /alerts/{id}/close
#[derive(Debug, Deserialize)]
pub struct CloseRequest {
    /// Human-readable reason for closing the alert.
    pub reason: Option<String>,
}

// ============================================================
// API response type
// ============================================================

#[derive(Debug, Serialize)]
pub struct AlertResponse {
    pub id:               Uuid,
    pub alert_type:       String,
    pub severity:         String,
    pub status:           String,
    pub source_domain:    String,
    pub source_entity_id: Option<Uuid>,
    pub title:            String,
    pub description:      Option<String>,
    pub payload:          serde_json::Value,
    pub acknowledged_by:  Option<Uuid>,
    pub acknowledged_at:  Option<DateTime<Utc>>,
    pub closed_by:        Option<Uuid>,
    pub closed_at:        Option<DateTime<Utc>>,
    pub close_reason:     Option<String>,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

impl From<AlertRow> for AlertResponse {
    fn from(r: AlertRow) -> Self {
        AlertResponse {
            id:               r.id,
            alert_type:       r.alert_type,
            severity:         r.severity,
            status:           r.status,
            source_domain:    r.source_domain,
            source_entity_id: r.source_entity_id,
            title:            r.title,
            description:      r.description,
            payload:          r.payload,
            acknowledged_by:  r.acknowledged_by,
            acknowledged_at:  r.acknowledged_at,
            closed_by:        r.closed_by,
            closed_at:        r.closed_at,
            close_reason:     r.close_reason,
            created_at:       r.created_at,
            updated_at:       r.updated_at,
        }
    }
}
