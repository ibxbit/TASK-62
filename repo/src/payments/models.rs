use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================
// DB row types
// ============================================================

use bigdecimal::{BigDecimal, ToPrimitive, FromPrimitive};
use std::str::FromStr;
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GatewayConfigRow {
    pub id:             Uuid,
    pub name:           String,
    pub display_name:   String,
    pub hmac_secret:    String,
    pub hmac_algorithm: String,
    pub sig_header:     String,
    pub nonce_header:   String,
    pub ts_header:      String,
    pub ts_in_sig:      bool,
    pub amount:         BigDecimal,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TransactionRow {
    pub id:                     Uuid,
    pub idempotency_key:        String,
    pub trip_id:                Option<Uuid>,
    pub route_id:               Option<Uuid>,
    pub amount:                 BigDecimal,
    pub currency:               String,
    pub payment_method:         String,
    pub status:                 String,
    pub collected_by:           Option<Uuid>,
    pub metadata:               serde_json::Value,
    pub payer_ref_encrypted:    Option<Vec<u8>>,
    pub created_at:             DateTime<Utc>,
    pub updated_at:             DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CallbackRow {
    pub id:                  Uuid,
    pub transaction_id:      Option<Uuid>,
    pub nonce:               String,
    pub signature:           String,
    pub payload_hash:        String,
    pub payload:             serde_json::Value,
    pub source:              String,
    pub received_at:         DateTime<Utc>,
    pub processed_at:        Option<DateTime<Utc>>,
    pub callback_timestamp:  Option<DateTime<Utc>>,
    pub status:              String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefundRow {
    pub id:               Uuid,
    pub transaction_id:   Uuid,
    pub idempotency_key:  String,
    pub amount:           BigDecimal,
    pub reason:           Option<String>,
    pub status:           String,
    pub requested_by:     Uuid,
    pub approved_by:      Option<Uuid>,
    pub processed_at:     Option<DateTime<Utc>>,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StatementImportRow {
    pub id:                Uuid,
    pub filename:          String,
    pub file_hash:         String,
    pub source:            String,
    pub import_date:       NaiveDate,
    pub status:            String,
    pub total_records:     i32,
    pub processed_records: i32,
    pub error_count:       i32,
    pub amount:          BigDecimal,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}


#[derive(Debug, Deserialize)]
pub struct CreateTransactionRequest {
    pub idempotency_key:  String,
    pub trip_id:          Option<Uuid>,
    pub route_id:         Option<Uuid>,
    pub amount:           f64,
    pub currency:         Option<String>,    // default "CNY"
    pub payment_method:   String,
    pub metadata:         Option<serde_json::Value>,
    /// Card last 4 digits — stored encrypted at rest, returned masked.
    pub card_last4:       Option<String>,
    /// Opaque payer reference — stored encrypted at rest; presence exposed as `has_payer_ref`.
    pub payer_ref:        Option<String>,
}

/// Inbound gateway callback body.
/// Providers typically POST a JSON payload; anti-replay fields come from HTTP headers.
#[derive(Debug, Deserialize)]
pub struct CallbackBody {
    /// Provider-reported transaction reference (used to match our transaction).
    pub transaction_ref: Option<String>,
    /// Provider-reported status (e.g. "SUCCESS", "FAILED").
    pub status:          Option<String>,
    /// Amount in provider's base unit (e.g. cents).
    pub amount:          Option<i64>,
    /// Arbitrary additional fields from the provider.
    #[serde(flatten)]
    pub extra:           serde_json::Value,
}

/// Anti-replay headers extracted from an inbound callback request.
#[derive(Debug, Clone)]
pub struct CallbackHeaders {
    pub signature:  String,
    pub nonce:      String,
    /// Unix timestamp (seconds) provided by the gateway.
    pub timestamp:  i64,
}

/// Request body for `POST /payments/callbacks/simulate`.
#[derive(Debug, Deserialize)]
pub struct SimulateCallbackRequest {
    pub gateway:         String,
    pub transaction_id:  Uuid,
    pub status:          String,    // e.g. "completed" | "failed"
    pub amount_cents:    Option<i64>,
}

/// Request body for `POST /payments/imports` (base64-encoded file content).
#[derive(Debug, Deserialize)]
pub struct UploadImportRequest {
    pub filename:       String,
    pub source:         String,
    pub format:         String,              // "csv" | "json"
    pub content_base64: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRefundRequest {
    pub transaction_id:  Uuid,
    pub idempotency_key: String,
    pub amount:          f64,
    pub reason:          Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListTransactionsQuery {
    pub status:   Option<String>,
    pub trip_id:  Option<Uuid>,
    pub limit:    Option<i64>,
    pub offset:   Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListRefundsQuery {
    pub status:          Option<String>,
    pub transaction_id:  Option<Uuid>,
    pub limit:           Option<i64>,
    pub offset:          Option<i64>,
}

// ============================================================
// API response types
// ============================================================

#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    pub id:              Uuid,
    pub idempotency_key: String,
    pub trip_id:         Option<Uuid>,
    pub route_id:        Option<Uuid>,
    pub amount:          f64,
    pub currency:        String,
    pub payment_method:  String,
    pub status:          String,
    /// Masked card last 4 digits, e.g. `"****1234"`. `null` if not provided at creation.
    pub card_last4:      Option<String>,
    /// `true` when a payer reference was stored; the reference itself is never returned.
    pub has_payer_ref:   bool,
    pub metadata:        serde_json::Value,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl TransactionResponse {
    pub fn from_row(r: TransactionRow, crypto: &crate::crypto::FieldEncryptor) -> Self {
        // TransactionRow does not have card_last4_encrypted, only payer_ref_encrypted
        let card_last4 = None;
        let has_payer_ref = r.payer_ref_encrypted.is_some();
        TransactionResponse {
            id:              r.id,
            idempotency_key: r.idempotency_key,
            trip_id:         r.trip_id,
            route_id:        r.route_id,
            amount:          r.amount.to_f64().unwrap_or(0.0),
            currency:        r.currency,
            payment_method:  r.payment_method,
            status:          r.status,
            card_last4,
            has_payer_ref,
            metadata:        r.metadata,
            created_at:      r.created_at,
            updated_at:      r.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CallbackResponse {
    pub id:                 Uuid,
    pub transaction_id:     Option<Uuid>,
    pub source:             String,
    pub status:             String,
    pub nonce:              String,
    pub callback_timestamp: Option<DateTime<Utc>>,
    pub received_at:        DateTime<Utc>,
    pub processed_at:       Option<DateTime<Utc>>,
}

impl From<CallbackRow> for CallbackResponse {
    fn from(r: CallbackRow) -> Self {
        CallbackResponse {
            id:                 r.id,
            transaction_id:     r.transaction_id,
            source:             r.source,
            status:             r.status,
            nonce:              r.nonce,
            callback_timestamp: r.callback_timestamp,
            received_at:        r.received_at,
            processed_at:       r.processed_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RefundResponse {
    pub id:               Uuid,
    pub transaction_id:   Uuid,
    pub idempotency_key:  String,
    pub amount:           f64,
    pub reason:           Option<String>,
    pub status:           String,
    pub requested_by:     Uuid,
    pub approved_by:      Option<Uuid>,
    pub processed_at:     Option<DateTime<Utc>>,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

impl From<RefundRow> for RefundResponse {
    fn from(r: RefundRow) -> Self {
        RefundResponse {
            id:               r.id,
            transaction_id:   r.transaction_id,
            idempotency_key:  r.idempotency_key,
            amount:           r.amount.to_f64().unwrap_or(0.0),
            reason:           r.reason,
            status:           r.status,
            requested_by:     r.requested_by,
            approved_by:      r.approved_by,
            processed_at:     r.processed_at,
            created_at:       r.created_at,
            updated_at:       r.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StatementImportResponse {
    pub id:                Uuid,
    pub filename:          String,
    pub source:            String,
    pub import_date:       NaiveDate,
    pub status:            String,
    pub total_records:     i32,
    pub processed_records: i32,
    pub error_count:       i32,
    pub created_at:        DateTime<Utc>,
}

impl From<StatementImportRow> for StatementImportResponse {
    fn from(r: StatementImportRow) -> Self {
        StatementImportResponse {
            id:                r.id,
            filename:          r.filename,
            source:            r.source,
            import_date:       r.import_date,
            status:            r.status,
            total_records:     r.total_records,
            processed_records: r.processed_records,
            error_count:       r.error_count,
            created_at:        r.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CompensationJobResponse {
    pub id:             Uuid,
    pub job_type:       String,
    pub status:         String,
    pub affected_count: i32,
    pub error_message:  Option<String>,
    pub started_at:     DateTime<Utc>,
    pub completed_at:   Option<DateTime<Utc>>,
}

// Removed invalid From<CompensationJobRow> for CompensationJobResponse
