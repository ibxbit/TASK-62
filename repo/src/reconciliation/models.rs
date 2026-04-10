use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================
// CSV statement record
// ============================================================

/// One row parsed from a daily bank/provider statement CSV.
#[derive(Debug, Clone)]
pub struct StatementRecord {
    /// The provider-assigned unique reference for this entry.
    pub reference:   String,
    pub amount:      f64,          // always positive
    pub entry_type:  EntryType,
    pub date:        NaiveDate,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    Debit,
    Credit,
}

impl EntryType {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryType::Debit  => "debit",
            EntryType::Credit => "credit",
        }
    }
}

// ============================================================
// Format validation
// ============================================================

/// Result returned by `importer::validate_and_parse`.
#[derive(Debug)]
pub struct ParseResult {
    /// True iff no fatal format errors were found.
    pub is_valid:   bool,
    /// List of human-readable validation errors (empty when `is_valid = true`).
    pub errors:     Vec<String>,
    /// Successfully parsed records (may be non-empty even when errors exist —
    /// partial results for diagnostic purposes).
    pub records:    Vec<StatementRecord>,
    /// SHA-256 of the raw file content, hex-encoded.
    pub fingerprint: String,
}

// ============================================================
// Discrepancy types
// ============================================================

/// The classification of each reconciliation item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscrepancyType {
    /// Statement and DB agree on both existence and amount.
    Matched,
    /// Both sides present but amount differs by more than the threshold ($0.01).
    AmountMismatch,
    /// DB transaction has no corresponding statement entry.
    MissingFromStatement,
    /// Statement entry has no corresponding DB transaction.
    ExtraInStatement,
    /// This statement entry is a duplicate of another entry for the same reference.
    Duplicate,
}

impl DiscrepancyType {
    pub fn as_str(self) -> &'static str {
        match self {
            DiscrepancyType::Matched               => "matched",
            DiscrepancyType::AmountMismatch        => "amount_mismatch",
            DiscrepancyType::MissingFromStatement  => "missing_from_statement",
            DiscrepancyType::ExtraInStatement      => "extra_in_statement",
            DiscrepancyType::Duplicate             => "duplicate",
        }
    }

    /// The existing `match_status` column value that maps to this discrepancy.
    pub fn match_status(self) -> &'static str {
        match self {
            DiscrepancyType::Matched               => "matched",
            DiscrepancyType::AmountMismatch        => "discrepancy",
            DiscrepancyType::MissingFromStatement  => "missing",
            DiscrepancyType::ExtraInStatement      => "extra",
            DiscrepancyType::Duplicate             => "discrepancy",
        }
    }
}

// ============================================================
// One reconciled item
// ============================================================

#[derive(Debug)]
pub struct ReconItem {
    pub transaction_id:    Option<Uuid>,
    pub statement_line_id: Option<Uuid>,
    /// Amount from DB transaction (0 if not found).
    pub expected_amount:   f64,
    /// Amount from statement (0 if not found).
    pub actual_amount:     f64,
    pub discrepancy_type:  DiscrepancyType,
    pub notes:             Option<String>,
}

// ============================================================
// Engine output
// ============================================================

#[derive(Debug)]
pub struct ReconciliationOutput {
    pub run_id:             Uuid,
    pub run_date:           NaiveDate,
    pub matched:            usize,
    pub amount_mismatches:  usize,
    pub missing_from_stmt:  usize,
    pub extra_in_stmt:      usize,
    pub duplicates:         usize,
    pub total_expected:     f64,
    pub total_collected:    f64,
    pub items:              Vec<ReconItem>,
}

impl ReconciliationOutput {
    pub fn discrepancy_count(&self) -> usize {
        self.amount_mismatches + self.missing_from_stmt + self.extra_in_stmt + self.duplicates
    }

    pub fn is_high_discrepancy(&self, total_records: usize) -> bool {
        let count = self.discrepancy_count();
        count > 10 || (total_records > 0 && count * 100 / total_records > 5)
    }
}

// ============================================================
// DB row types
// ============================================================

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReconciliationRunRow {
    pub id:                   Uuid,
    pub run_date:             NaiveDate,
    pub status:               String,
    pub statement_import_id:  Option<Uuid>,
    pub total_expected:       f64,
    pub total_collected:      f64,
    pub discrepancy_count:    i32,
    pub started_at:           Option<DateTime<Utc>>,
    pub completed_at:         Option<DateTime<Utc>>,
    pub run_by:               Option<Uuid>,
    pub notes:                Option<String>,
    pub created_at:           DateTime<Utc>,
    pub updated_at:           DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReconciliationItemRow {
    pub id:                Uuid,
    pub run_id:            Uuid,
    pub transaction_id:    Option<Uuid>,
    pub expected_amount:   f64,
    pub actual_amount:     f64,
    pub match_status:      String,
    pub discrepancy_type:  Option<String>,
    pub notes:             Option<String>,
    pub created_at:        DateTime<Utc>,
}

// ============================================================
// API request / response types
// ============================================================

/// POST /reconciliation/runs
#[derive(Debug, Deserialize)]
pub struct StartRunRequest {
    /// The calendar date to reconcile.  Required for actual runs; missing
    /// date is caught by the handler with a 400 rather than a 422 so that
    /// the RBAC / reauth check fires first.
    #[serde(default)]
    pub run_date:             Option<NaiveDate>,
    /// UUID of the previously uploaded statement import.
    /// Accepts the short alias `statement_id` for API client convenience.
    #[serde(alias = "statement_id")]
    pub statement_import_id:  Uuid,
    /// Optional SHA-256 fingerprint the caller expects the file to have.
    /// When provided, verified before reconciliation begins.
    pub expected_fingerprint: Option<String>,
}

/// GET /reconciliation/runs query params
#[derive(Debug, Deserialize)]
pub struct ListRunsQuery {
    pub status: Option<String>,
    pub limit:  Option<i64>,
    pub offset: Option<i64>,
}

/// GET /reconciliation/runs/{id}/items query params
#[derive(Debug, Deserialize)]
pub struct ListItemsQuery {
    pub discrepancy_type: Option<String>,
    pub limit:            Option<i64>,
    pub offset:           Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub id:                  Uuid,
    pub run_date:            NaiveDate,
    pub status:              String,
    pub statement_import_id: Option<Uuid>,
    pub total_expected:      f64,
    pub total_collected:     f64,
    pub total_discrepancy:   f64,
    pub discrepancy_count:   i32,
    pub started_at:          Option<DateTime<Utc>>,
    pub completed_at:        Option<DateTime<Utc>>,
    pub run_by:              Option<Uuid>,
    pub notes:               Option<String>,
    pub created_at:          DateTime<Utc>,
}

impl From<ReconciliationRunRow> for RunResponse {
    fn from(r: ReconciliationRunRow) -> Self {
        RunResponse {
            total_discrepancy: r.total_collected - r.total_expected,
            id:                  r.id,
            run_date:            r.run_date,
            status:              r.status,
            statement_import_id: r.statement_import_id,
            total_expected:      r.total_expected,
            total_collected:     r.total_collected,
            discrepancy_count:   r.discrepancy_count,
            started_at:          r.started_at,
            completed_at:        r.completed_at,
            run_by:              r.run_by,
            notes:               r.notes,
            created_at:          r.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ItemResponse {
    pub id:               Uuid,
    pub run_id:           Uuid,
    pub transaction_id:   Option<Uuid>,
    pub expected_amount:  f64,
    pub actual_amount:    f64,
    pub discrepancy:      f64,
    pub match_status:     String,
    pub discrepancy_type: Option<String>,
    pub notes:            Option<String>,
    pub created_at:       DateTime<Utc>,
}

impl From<ReconciliationItemRow> for ItemResponse {
    fn from(r: ReconciliationItemRow) -> Self {
        ItemResponse {
            discrepancy:      r.actual_amount - r.expected_amount,
            id:               r.id,
            run_id:           r.run_id,
            transaction_id:   r.transaction_id,
            expected_amount:  r.expected_amount,
            actual_amount:    r.actual_amount,
            match_status:     r.match_status,
            discrepancy_type: r.discrepancy_type,
            notes:            r.notes,
            created_at:       r.created_at,
        }
    }
}

/// Response from uploading a statement CSV.
#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub import_id:    Uuid,
    pub fingerprint:  String,
    pub record_count: usize,
    pub is_valid:     bool,
    pub errors:       Vec<String>,
}
