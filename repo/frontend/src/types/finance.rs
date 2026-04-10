use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A payment transaction.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct Transaction {
    pub id:              Uuid,
    pub idempotency_key: String,
    pub amount:          f64,
    pub currency:        String,
    pub payment_method:  String,
    pub status:          String,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl Transaction {
    pub fn status_label(&self) -> &str {
        match self.status.as_str() {
            "pending"           => "Pending",
            "completed"         => "Completed",
            "failed"            => "Failed",
            "partially_refunded" => "Partially Refunded",
            "refunded"          => "Refunded",
            other               => other,
        }
    }
}

/// A statement import record.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct StatementImport {
    pub id:            Uuid,
    pub filename:      String,
    pub source:        String,
    pub import_date:   NaiveDate,
    pub status:        String,
    pub total_records: i32,
    pub file_hash:     String,
    pub created_at:    DateTime<Utc>,
}

/// A reconciliation run.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct ReconciliationRun {
    pub id:                  Uuid,
    pub run_date:            NaiveDate,
    pub status:              String,
    pub statement_import_id: Uuid,
    pub total_expected:      Option<f64>,
    pub total_collected:     Option<f64>,
    pub discrepancy_count:   i32,
    pub started_at:          DateTime<Utc>,
    pub completed_at:        Option<DateTime<Utc>>,
    pub created_at:          DateTime<Utc>,
}

impl ReconciliationRun {
    pub fn net_discrepancy(&self) -> f64 {
        match (self.total_collected, self.total_expected) {
            (Some(c), Some(e)) => c - e,
            _ => 0.0,
        }
    }
}

/// Summary of a reconciliation run.
#[derive(Clone, PartialEq, Deserialize, Debug, Default)]
pub struct RunSummary {
    pub run_id:            Uuid,
    pub total_expected:    Option<f64>,
    pub total_collected:   Option<f64>,
    pub total_discrepancy: Option<f64>,
    pub discrepancy_count: i32,
    pub amount_tolerance:  f64,
    pub by_type:           serde_json::Value,
}

/// A refund record.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct Refund {
    pub id:              Uuid,
    pub transaction_id:  Uuid,
    pub idempotency_key: String,
    pub amount:          f64,
    pub reason:          Option<String>,
    pub status:          String,
    pub created_at:      DateTime<Utc>,
}

impl Refund {
    pub fn status_label(&self) -> &str {
        match self.status.as_str() {
            "pending"   => "Pending",
            "approved"  => "Approved",
            "completed" => "Completed",
            "rejected"  => "Rejected",
            other       => other,
        }
    }
    pub fn can_approve(&self) -> bool  { self.status == "pending" }
    pub fn can_process(&self) -> bool  { self.status == "approved" }
}

/// Request to upload a statement.
#[derive(Serialize)]
pub struct UploadStatementRequest {
    pub filename:      String,
    pub source:        String,
    pub content_base64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_fingerprint: Option<String>,
}

/// Response from statement upload.
#[derive(Deserialize)]
pub struct UploadStatementResponse {
    pub import_id:    Uuid,
    pub fingerprint:  String,
    pub record_count: usize,
    pub is_valid:     bool,
    pub errors:       Vec<String>,
}

/// Request to start a reconciliation run.
#[derive(Serialize)]
pub struct StartRunRequest {
    pub statement_import_id: Uuid,
    pub run_date:            String,
}
