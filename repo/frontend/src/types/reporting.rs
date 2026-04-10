use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A KPI/metric definition.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct MetricDefinition {
    pub id:             Uuid,
    pub metric_key:     String,
    pub display_name:   String,
    pub description:    Option<String>,
    pub formula_type:   String,
    pub dimension_keys: Vec<String>,
    pub config:         serde_json::Value,
    pub is_builtin:     bool,
    pub is_active:      bool,
    pub created_at:     DateTime<Utc>,
}

impl MetricDefinition {
    pub fn formula_label(&self) -> &str {
        match self.formula_type.as_str() {
            "on_time_departure_rate"       => "On-time Departure Rate",
            "refund_rate"                  => "Refund Rate",
            "reconciliation_mismatch_count" => "Reconciliation Mismatches",
            "custom_sql"                   => "Custom SQL",
            other                          => other,
        }
    }
}

/// A scheduled report definition.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct ScheduledReport {
    pub id:              Uuid,
    pub name:            String,
    pub metric_ids:      Vec<Uuid>,
    pub schedule:        String,
    pub date_range_days: i32,
    pub granularity:     String,
    pub output_format:   String,
    pub is_active:       bool,
    pub next_run_at:     Option<DateTime<Utc>>,
    pub last_run_at:     Option<DateTime<Utc>>,
    pub created_at:      DateTime<Utc>,
}

/// A report run result.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct ReportRun {
    pub id:           Uuid,
    pub scheduled_id: Option<Uuid>,
    pub status:       String,
    pub date_from:    DateTime<Utc>,
    pub date_to:      DateTime<Utc>,
    pub output_format: String,
    pub result_data:  Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub started_at:   DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at:   DateTime<Utc>,
}

impl ReportRun {
    pub fn is_completed(&self) -> bool { self.status == "completed" }
    pub fn is_running(&self)   -> bool { self.status == "running" }
    pub fn is_failed(&self)    -> bool { self.status == "failed" }
}

/// A computed metric data point for a specific period.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct MetricValue {
    pub metric_id:    Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end:   DateTime<Utc>,
    pub value:        f64,
    pub dimensions:   serde_json::Value,
}

/// Request to create a metric.
#[derive(Serialize, Default)]
pub struct CreateMetricRequest {
    pub metric_key:   String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:  Option<String>,
    pub formula_type: String,
}

/// Request to create a scheduled report.
#[derive(Serialize, Default)]
pub struct CreateScheduledReportRequest {
    pub name:       String,
    pub metric_ids: Vec<Uuid>,
    pub schedule:   String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_range_days: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
}
