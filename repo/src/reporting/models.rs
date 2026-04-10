use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// ============================================================
// DB row types
// ============================================================

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MetricDefinitionRow {
    pub id:             Uuid,
    pub metric_key:     String,
    pub display_name:   String,
    pub description:    Option<String>,
    pub formula_type:   String,
    pub dimension_keys: Vec<String>,
    pub config:         Value,
    pub is_builtin:     bool,
    pub is_active:      bool,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MetricSnapshotRow {
    pub id:           Uuid,
    pub metric_id:    Uuid,
    pub granularity:  String,
    pub period_start: DateTime<Utc>,
    pub period_end:   DateTime<Utc>,
    pub route_id:     Option<Uuid>,
    pub depot_id:     Option<Uuid>,
    pub value:        f64,
    pub sample_count: i64,
    pub computed_at:  DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScheduledReportRow {
    pub id:                 Uuid,
    pub name:               String,
    pub metric_ids:         Vec<Uuid>,
    pub schedule:           String,
    pub route_id:           Option<Uuid>,
    pub depot_id:           Option<Uuid>,
    pub date_range_days:    i32,
    pub granularity:        String,
    pub output_format:      String,
    pub recipient_user_ids: Vec<Uuid>,
    pub is_active:          bool,
    pub next_run_at:        DateTime<Utc>,
    pub last_run_at:        Option<DateTime<Utc>>,
    pub created_by:         Uuid,
    pub created_at:         DateTime<Utc>,
    pub updated_at:         DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReportRunRow {
    pub id:              Uuid,
    pub scheduled_id:    Option<Uuid>,
    pub trigger_user_id: Option<Uuid>,
    pub metric_ids:      Vec<Uuid>,
    pub route_id:        Option<Uuid>,
    pub depot_id:        Option<Uuid>,
    pub date_from:       DateTime<Utc>,
    pub date_to:         DateTime<Utc>,
    pub granularity:     String,
    pub output_format:   String,
    pub status:          String,
    pub result_data:     Option<Value>,
    pub error_message:   Option<String>,
    pub started_at:      Option<DateTime<Utc>>,
    pub completed_at:    Option<DateTime<Utc>>,
    pub created_at:      DateTime<Utc>,
}

// ============================================================
// Metric computation types
// ============================================================

/// One data point in a time-series result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub period_start:  DateTime<Utc>,
    pub period_end:    DateTime<Utc>,
    pub value:         f64,
    pub sample_count:  i64,
}

/// Full result for a single metric over a query range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricResult {
    pub metric_key:   String,
    pub display_name: String,
    pub unit:         String,
    pub series:       Vec<TimeSeriesPoint>,
    pub summary:      MetricSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSummary {
    pub total_samples: i64,
    pub average:       f64,
    pub min:           f64,
    pub max:           f64,
}

/// Parameters for computing a metric.
#[derive(Debug, Clone, Deserialize)]
pub struct MetricQuery {
    pub metric_id:   Uuid,
    pub date_from:   DateTime<Utc>,
    pub date_to:     DateTime<Utc>,
    pub granularity: String,
    pub route_id:    Option<Uuid>,
    pub depot_id:    Option<Uuid>,
}

// ============================================================
// API request / response types
// ============================================================

#[derive(Debug, Deserialize)]
pub struct CreateMetricRequest {
    /// Unique key for this metric. Accepts `name` as an alias.
    #[serde(alias = "name")]
    pub metric_key:     String,
    /// Human-readable display name. Defaults to empty string when omitted;
    /// the handler validates it is non-empty before inserting.
    #[serde(default)]
    pub display_name:   String,
    pub description:    Option<String>,
    /// Formula type. Defaults to empty string when omitted;
    /// the handler validates it is one of the supported types.
    #[serde(default)]
    pub formula_type:   String,
    pub dimension_keys: Option<Vec<String>>,
    pub config:         Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMetricRequest {
    pub display_name:   Option<String>,
    pub description:    Option<String>,
    pub dimension_keys: Option<Vec<String>>,
    pub config:         Option<Value>,
    pub is_active:      Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct MetricDefinitionResponse {
    pub id:             Uuid,
    pub metric_key:     String,
    pub display_name:   String,
    pub description:    Option<String>,
    pub formula_type:   String,
    pub dimension_keys: Vec<String>,
    pub config:         Value,
    pub is_builtin:     bool,
    pub is_active:      bool,
    pub created_at:     DateTime<Utc>,
}

impl From<MetricDefinitionRow> for MetricDefinitionResponse {
    fn from(r: MetricDefinitionRow) -> Self {
        MetricDefinitionResponse {
            id:             r.id,
            metric_key:     r.metric_key,
            display_name:   r.display_name,
            description:    r.description,
            formula_type:   r.formula_type,
            dimension_keys: r.dimension_keys,
            config:         r.config,
            is_builtin:     r.is_builtin,
            is_active:      r.is_active,
            created_at:     r.created_at,
        }
    }
}

// ---- Compute request ----

#[derive(Debug, Deserialize)]
pub struct ComputeRequest {
    pub metric_ids:  Vec<Uuid>,
    pub date_from:   DateTime<Utc>,
    pub date_to:     DateTime<Utc>,
    pub granularity: Option<String>,   // default "day"
    pub route_id:    Option<Uuid>,
    pub depot_id:    Option<Uuid>,
}

// ---- Scheduled report requests ----

#[derive(Debug, Deserialize)]
pub struct CreateScheduledReportRequest {
    pub name:               String,
    pub metric_ids:         Vec<Uuid>,
    pub schedule:           String,
    pub route_id:           Option<Uuid>,
    pub depot_id:           Option<Uuid>,
    pub date_range_days:    Option<i32>,
    pub granularity:        Option<String>,
    pub output_format:      Option<String>,
    pub recipient_user_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduledReportRequest {
    pub name:               Option<String>,
    pub metric_ids:         Option<Vec<Uuid>>,
    pub schedule:           Option<String>,
    pub route_id:           Option<Uuid>,
    pub depot_id:           Option<Uuid>,
    pub date_range_days:    Option<i32>,
    pub granularity:        Option<String>,
    pub output_format:      Option<String>,
    pub recipient_user_ids: Option<Vec<Uuid>>,
    pub is_active:          Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ScheduledReportResponse {
    pub id:                 Uuid,
    pub name:               String,
    pub metric_ids:         Vec<Uuid>,
    pub schedule:           String,
    pub route_id:           Option<Uuid>,
    pub depot_id:           Option<Uuid>,
    pub date_range_days:    i32,
    pub granularity:        String,
    pub output_format:      String,
    pub recipient_user_ids: Vec<Uuid>,
    pub is_active:          bool,
    pub next_run_at:        DateTime<Utc>,
    pub last_run_at:        Option<DateTime<Utc>>,
    pub created_by:         Uuid,
    pub created_at:         DateTime<Utc>,
}

impl From<ScheduledReportRow> for ScheduledReportResponse {
    fn from(r: ScheduledReportRow) -> Self {
        ScheduledReportResponse {
            id:                 r.id,
            name:               r.name,
            metric_ids:         r.metric_ids,
            schedule:           r.schedule,
            route_id:           r.route_id,
            depot_id:           r.depot_id,
            date_range_days:    r.date_range_days,
            granularity:        r.granularity,
            output_format:      r.output_format,
            recipient_user_ids: r.recipient_user_ids,
            is_active:          r.is_active,
            next_run_at:        r.next_run_at,
            last_run_at:        r.last_run_at,
            created_by:         r.created_by,
            created_at:         r.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReportRunResponse {
    pub id:            Uuid,
    pub scheduled_id:  Option<Uuid>,
    pub metric_ids:    Vec<Uuid>,
    pub route_id:      Option<Uuid>,
    pub depot_id:      Option<Uuid>,
    pub date_from:     DateTime<Utc>,
    pub date_to:       DateTime<Utc>,
    pub granularity:   String,
    pub output_format: String,
    pub status:        String,
    pub started_at:    Option<DateTime<Utc>>,
    pub completed_at:  Option<DateTime<Utc>>,
    pub created_at:    DateTime<Utc>,
}

impl From<ReportRunRow> for ReportRunResponse {
    fn from(r: ReportRunRow) -> Self {
        ReportRunResponse {
            id:            r.id,
            scheduled_id:  r.scheduled_id,
            metric_ids:    r.metric_ids,
            route_id:      r.route_id,
            depot_id:      r.depot_id,
            date_from:     r.date_from,
            date_to:       r.date_to,
            granularity:   r.granularity,
            output_format: r.output_format,
            status:        r.status,
            started_at:    r.started_at,
            completed_at:  r.completed_at,
            created_at:    r.created_at,
        }
    }
}

/// Query parameters for the export endpoint.
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub format: Option<String>,   // "csv" | "pdf"; default "csv"
}
