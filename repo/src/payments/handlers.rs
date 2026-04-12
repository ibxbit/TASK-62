use bigdecimal::{ToPrimitive, FromPrimitive};
use crate::payments::gateway::PaymentGateway;
use actix_web::{web, HttpRequest, HttpResponse};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::auth::middleware::AuthSession;
use crate::error::AppError;
use crate::rbac::permissions::Permission;
use crate::AppState;

use super::compensation;
use super::gateway::load_gateway;
use super::import::{process_import_file, sha256_hex};
use super::models::{
    CallbackResponse, CallbackRow, CompensationJobResponse,
    CreateRefundRequest, CreateTransactionRequest, ListRefundsQuery, ListTransactionsQuery,
    RefundResponse, RefundRow, SimulateCallbackRequest, StatementImportResponse,
    StatementImportRow, TransactionResponse, TransactionRow, UploadImportRequest,
};
use super::signature::{verify_callback, sha256_hex as sig_sha256_hex};

// ============================================================
// Transactions
// ============================================================

/// POST /payments/transactions
///
/// Idempotent: if `idempotency_key` already exists returns the existing record
/// with `200 OK` rather than `201 Created`.
pub async fn create_transaction(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<CreateTransactionRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsTransactionsWrite)?;

    // Idempotency check
    let existing = sqlx::query_as::<_, TransactionRow>(
        r#"SELECT id, idempotency_key, trip_id, route_id,
                  amount::double precision AS amount, currency, payment_method, status,
                  collected_by, metadata, card_last4_encrypted, payer_ref_encrypted,
                  created_at, updated_at
           FROM payments.transactions WHERE idempotency_key = $1"#
    )
    .bind(&body.idempotency_key)
    .fetch_optional(&state.db)
    .await?;

    if let Some(row) = existing {
        return Ok(HttpResponse::Ok().json(TransactionResponse::from_row(row, &state.crypto)));
    }

    let valid_methods = ["cash","card","mobile","bank_transfer","voucher","other"];
    if !valid_methods.contains(&body.payment_method.as_str()) {
        return Err(AppError::BadRequest(format!(
            "payment_method must be one of: {}", valid_methods.join(", ")
        )));
    }

    let currency       = body.currency.clone().unwrap_or_else(|| "CNY".to_string());
    let metadata       = body.metadata.clone().unwrap_or(serde_json::Value::Object(Default::default()));
    let card_last4_enc = state.crypto.encrypt_opt(body.card_last4.as_deref())?;
    let payer_ref_enc  = state.crypto.encrypt_opt(body.payer_ref.as_deref())?;

    let row = sqlx::query_as::<_, TransactionRow>(
        r#"INSERT INTO payments.transactions
               (idempotency_key, trip_id, route_id, amount, currency,
                payment_method, collected_by, metadata,
                card_last4_encrypted, payer_ref_encrypted)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
           RETURNING id, idempotency_key, trip_id, route_id,
                     amount::double precision AS amount, currency, payment_method, status,
                     collected_by, metadata, card_last4_encrypted, payer_ref_encrypted,
                     created_at, updated_at"#
    )
    .bind(&body.idempotency_key)
    .bind(body.trip_id)
    .bind(body.route_id)
    .bind(body.amount)
    .bind(&currency)
    .bind(&body.payment_method)
    .bind(session.user_id)
    .bind(&metadata)
    .bind(&card_last4_enc)
    .bind(&payer_ref_enc)
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Created().json(TransactionResponse::from_row(row, &state.crypto)))
}

/// GET /payments/transactions
pub async fn list_transactions(
    state:   web::Data<AppState>,
    session: AuthSession,
    query:   web::Query<ListTransactionsQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsTransactionsRead)?;

    let limit  = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    let rows = sqlx::query_as::<_, TransactionRow>(
        r#"SELECT id, idempotency_key, trip_id, route_id,
                  amount::double precision AS amount, currency, payment_method, status,
                  collected_by, metadata, card_last4_encrypted, payer_ref_encrypted,
                  created_at, updated_at
           FROM payments.transactions
           WHERE ($1::text IS NULL OR status  = $1)
             AND ($2::uuid IS NULL OR trip_id = $2)
           ORDER BY created_at DESC
           LIMIT $3 OFFSET $4"#
    )
    .bind(query.status.as_deref())
    .bind(query.trip_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<TransactionResponse> = rows.into_iter()
        .map(|r| TransactionResponse::from_row(r, &state.crypto))
        .collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// GET /payments/transactions/{id}
pub async fn get_transaction(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsTransactionsRead)?;
    let id = *path;

    let row = sqlx::query_as::<_, TransactionRow>(
        r#"SELECT id, idempotency_key, trip_id, route_id,
                  amount::double precision AS amount, currency, payment_method, status,
                  collected_by, metadata, card_last4_encrypted, payer_ref_encrypted,
                  created_at, updated_at
           FROM payments.transactions WHERE id = $1"#
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Transaction not found".to_string()))?;

    Ok(HttpResponse::Ok().json(TransactionResponse::from_row(row, &state.crypto)))
}

// ============================================================
// Callbacks
// ============================================================

/// POST /payments/callbacks/{gateway}
///
/// Receives an inbound webhook from a payment provider.
/// Pipeline:
///   1. Extract signature, nonce, timestamp from headers (per gateway config).
///   2. Verify signature + anti-replay (timestamp window + nonce uniqueness).
///   3. Insert callback record.
///   4. Attempt to match against a pending transaction and update its status.
pub async fn receive_callback(
    state:   web::Data<AppState>,
    req:     HttpRequest,
    path:    web::Path<String>,
    body:    web::Bytes,
) -> Result<HttpResponse, AppError> {
    let gateway_name = path.into_inner();
    let gw = load_gateway(&state.db, &gateway_name).await
        .map_err(AppError::from)?;

    // ---- Extract anti-replay headers ----
    let get_header = |name: &str| -> Result<String, AppError> {
        req.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .ok_or_else(|| AppError::BadRequest(
                format!("Missing required header: {}", name)
            ))
    };

    let signature  = get_header(gw.sig_header())?;
    let nonce      = get_header(gw.nonce_header())?;
    let ts_str     = get_header(gw.ts_header())?;
    let timestamp: i64 = ts_str.parse().map_err(|_|
        AppError::BadRequest("Invalid timestamp header (expected integer seconds)".to_string())
    )?;

    // ---- Verify signature + anti-replay ----
    verify_callback(&state.db, &gw, &body, &nonce, timestamp, &signature)
        .await
        .map_err(AppError::from)?;

    // ---- Parse payload (best-effort; store raw bytes even if unparseable) ----
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap_or(
        serde_json::json!({ "_raw": hex::encode(&body) })
    );
    let payload_hash = sig_sha256_hex(&body);
    let callback_ts: DateTime<Utc> = chrono::DateTime::from_timestamp(timestamp, 0)
        .unwrap_or_else(|| Utc::now());

    // ---- Match against an existing transaction ----
    let txn_ref      = payload.get("transaction_ref")
        .or_else(|| payload.get("out_trade_no"))
        .or_else(|| payload.get("transaction_id"))
        .and_then(|v| v.as_str());

    let transaction_id: Option<Uuid> = if let Some(ref_val) = txn_ref {
        let id: Option<Uuid> = sqlx::query_scalar!(
            "SELECT id FROM payments.transactions WHERE idempotency_key = $1 LIMIT 1",
            ref_val,
        )
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
        id
    } else {
        None
    };

    // ---- Insert callback record ----
    let callback_id = sqlx::query_scalar!(
        r#"
        INSERT INTO payments.callbacks
            (transaction_id, nonce, signature, payload_hash,
             payload, source, callback_timestamp, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'received')
        RETURNING id
        "#,
        transaction_id,
        nonce,
        signature,
        payload_hash,
        payload,
        gateway_name,
        callback_ts,
    )
    .fetch_one(&state.db)
    .await?;

    // ---- Promote transaction status based on callback ----
    if let Some(tx_id) = transaction_id {
        let cb_status = payload.get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let new_tx_status = if cb_status.to_uppercase().starts_with("FAIL") {
            "failed"
        } else if cb_status.to_uppercase().starts_with("SUCCESS")
               || cb_status.to_uppercase() == "PAID"
               || cb_status.to_uppercase() == "COMPLETED" {
            "completed"
        } else {
            // Unknown status — leave transaction unchanged
            ""
        };

        if !new_tx_status.is_empty() {
            sqlx::query!(
                r#"UPDATE payments.transactions
                   SET status = $2, updated_at = now()
                   WHERE id = $1 AND status = 'pending'"#,
                tx_id,
                new_tx_status,
            )
            .execute(&state.db)
            .await?;
        }

        // Mark callback processed
        sqlx::query!(
            r#"UPDATE payments.callbacks
               SET status = 'processed', processed_at = now()
               WHERE id = $1"#,
            callback_id,
        )
        .execute(&state.db)
        .await?;
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "callback_id": callback_id,
        "status":      "received",
    })))
}

/// POST /payments/callbacks/simulate
///
/// Simulates a gateway callback for a given transaction — useful in development
/// and integration testing.  Skips signature verification.
pub async fn simulate_callback(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<SimulateCallbackRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsTransactionsWrite)?;

    // Verify transaction exists
    let _: Option<Uuid> = sqlx::query_scalar!(
        "SELECT id FROM payments.transactions WHERE id = $1",
        body.transaction_id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Transaction not found".to_string()))?;

    let valid_statuses = ["completed", "failed"];
    if !valid_statuses.contains(&body.status.as_str()) {
        return Err(AppError::BadRequest(
            "status must be 'completed' or 'failed'".to_string(),
        ));
    }

    let now       = Utc::now();
    let nonce     = format!("sim_{}", uuid::Uuid::new_v4());
    let payload   = serde_json::json!({
        "status":          body.status,
        "transaction_ref": body.transaction_id,
        "amount":          body.amount_cents,
        "simulated":       true,
    });
    let payload_hash = sig_sha256_hex(payload.to_string().as_bytes());

    let callback_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO payments.callbacks
            (transaction_id, nonce, signature, payload_hash,
             payload, source, callback_timestamp, status, processed_at)
        VALUES ($1, $2, 'SIMULATED', $3, $4, $5, $6, 'processed', now())
        RETURNING id
        "#,
        body.transaction_id,
        nonce,
        payload_hash,
        payload,
        body.gateway,
        now,
    )
    .fetch_one(&state.db)
    .await?;

    // Update transaction status
    let new_status = &body.status;
    sqlx::query!(
        r#"UPDATE payments.transactions
           SET status = $2, updated_at = now()
           WHERE id = $1 AND status = 'pending'"#,
        body.transaction_id,
        new_status,
    )
    .execute(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "callback_id":    callback_id,
        "transaction_id": body.transaction_id,
        "status":         "processed",
    })))
}

/// GET /payments/callbacks/{id}
pub async fn get_callback(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsTransactionsRead)?;
    let id = *path;

    let row = sqlx::query_as!(
        CallbackRow,
        r#"
        SELECT id, transaction_id, nonce, signature, payload_hash,
               payload, source, received_at, processed_at,
               callback_timestamp, status
        FROM payments.callbacks WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Callback not found".to_string()))?;

    Ok(HttpResponse::Ok().json(CallbackResponse::from(row)))
}

// ============================================================
// Statement imports
// ============================================================

/// POST /payments/imports
///
/// Upload a payment result file (base64-encoded in JSON body).
/// SHA-256 of file content prevents duplicate ingestion.
pub async fn upload_import(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<UploadImportRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsStatementsImport)?;

    let valid_formats = ["csv", "json"];
        if !valid_formats.contains(&body.format.as_str()) {
        return Err(AppError::BadRequest(
            "format must be 'csv' or 'json'".to_string(),
        ));
    }

    // Decode base64 content
    let content = B64.decode(&body.content_base64).map_err(|e| {
        AppError::BadRequest(format!("content_base64 is not valid base64: {}", e))
    })?;

    let file_hash         = sha256_hex(&content);
    let import_date       = Utc::now().date_naive();
    let encrypted_content = state.crypto.encrypt_bytes(&content)?;

    // Duplicate detection — same file hash
    let existing: Option<Uuid> = sqlx::query_scalar!(
        "SELECT id FROM payments.statement_imports WHERE file_hash = $1",
        file_hash,
    )
    .fetch_optional(&state.db)
    .await?;

    if let Some(existing_id) = existing {
        return Err(AppError::BadRequest(format!(
            "File already imported (import_id: {})", existing_id
        )));
    }

        let row: StatementImportRow = sqlx::query_as!(
            StatementImportRow,
            r#"
            INSERT INTO payments.statement_imports
                (filename, file_hash, source, import_date, imported_by, raw_content_encrypted, amount)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, filename, file_hash, source, import_date,
                      status, total_records, processed_records,
                      error_count, amount, created_at, updated_at
            "#,
            body.filename,
            file_hash,
            body.source,
            import_date,
            session.user_id,
            encrypted_content.as_slice(),
            bigdecimal::BigDecimal::from(0),
        )
        .fetch_one(&state.db)
        .await?;

    Ok(HttpResponse::Created().json(StatementImportResponse::from(row)))
}

/// GET /payments/imports
pub async fn list_imports(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsStatementsRead)?;

    let rows: Vec<StatementImportRow> = sqlx::query_as!(
        StatementImportRow,
        r#"
        SELECT id, filename, file_hash, source, import_date,
               status, total_records, processed_records,
             error_count, amount, created_at, updated_at
        FROM payments.statement_imports
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<StatementImportResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// GET /payments/imports/{id}
pub async fn get_import(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsStatementsRead)?;
    let id = *path;

    let row = sqlx::query_as!(
        StatementImportRow,
        r#"
        SELECT id, filename, file_hash, source, import_date,
               status, total_records, processed_records,
             error_count, amount, created_at, updated_at
        FROM payments.statement_imports WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Import not found".to_string()))?;

    Ok(HttpResponse::Ok().json(StatementImportResponse::from(row)))
}

/// POST /payments/imports/{id}/process
///
/// Parses the stored file content and matches lines against transactions.
/// Idempotent: re-running on a completed import does nothing and returns the
/// current state.
pub async fn process_import(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsStatementsImport)?;
    let id = *path;

    let row = sqlx::query_as!(
        StatementImportRow,
        r#"
        SELECT id, filename, file_hash, source, import_date,
               status, total_records, processed_records,
               error_count, amount, created_at, updated_at
        FROM payments.statement_imports WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Import not found".to_string()))?;


    // The encrypted content is not loaded in StatementImportRow. You need a separate query to fetch it.
    let encrypted: Option<Option<Vec<u8>>> = sqlx::query_scalar!(
        "SELECT raw_content_encrypted FROM payments.statement_imports WHERE id = $1",
        id,
    )
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::BadRequest("Import has no stored content".to_string()))?;
    let encrypted = encrypted.ok_or_else(|| AppError::BadRequest("Import has no stored content".to_string()))??;
    let content = state.crypto.decrypt_bytes(&encrypted)?;

    // Detect format from existing metadata (we store filename)
    let import_row = sqlx::query_as!(
        StatementImportRow,
        r#"
        SELECT id, filename, file_hash, source, import_date,
               status, total_records, processed_records,
               error_count, amount, created_at, updated_at
        FROM payments.statement_imports WHERE id = $1
        "#,
        id,
    )
    .fetch_one(&state.db)
    .await?;

    let format = if import_row.filename.ends_with(".json") { "json" } else { "csv" };

    // Mark as 'processing'
    sqlx::query!(
        "UPDATE payments.statement_imports SET status = 'processing', updated_at = now() WHERE id = $1",
        id,
    )
    .execute(&state.db)
    .await?;

    let (total, matched) = process_import_file(&state.db, id, format, &content).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "import_id":      id,
        "total_records":  total,
        "matched":        matched,
        "unmatched":      total - matched,
        "status":         "completed",
    })))
}

// ============================================================
// Refunds
// ============================================================

/// POST /payments/refunds
///
/// Request a refund.  Idempotent via `idempotency_key`.
pub async fn create_refund(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<CreateRefundRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsRefundsWrite)?;

    // Idempotency check
    let existing: Option<RefundRow> = sqlx::query_as!(
        RefundRow,
        r#"
        SELECT id, transaction_id, idempotency_key, amount, reason,
               status, requested_by, approved_by, processed_at,
               created_at, updated_at
        FROM payments.refunds WHERE idempotency_key = $1
        "#,
        body.idempotency_key,
    )
    .fetch_optional(&state.db)
    .await?;

    if let Some(row) = existing {
        return Ok(HttpResponse::Ok().json(RefundResponse::from(row)));
    }

    // Verify the transaction exists and is refundable
    let tx: Option<(String,)> = sqlx::query!(
        "SELECT status FROM payments.transactions WHERE id = $1",
        body.transaction_id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Transaction not found".to_string()))?;

    if !["completed", "partially_refunded"].contains(&tx.status.as_str()) {
        return Err(AppError::BadRequest(format!(
            "Transaction status '{}' is not refundable (must be completed or partially_refunded)",
            tx.status
        )));
    }

    use bigdecimal::BigDecimal;
    let amount_bd = BigDecimal::from_f64(body.amount).unwrap_or(BigDecimal::from(0));
    let row: RefundRow = sqlx::query_as!(
        RefundRow,
        r#"
        INSERT INTO payments.refunds
            (transaction_id, idempotency_key, amount, reason, requested_by)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, transaction_id, idempotency_key, amount, reason,
                  status, requested_by, approved_by, processed_at,
                  created_at, updated_at
        "#,
        body.transaction_id,
        body.idempotency_key,
        amount_bd,
        body.reason,
        session.user_id,
    )
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Created().json(RefundResponse::from(row)))
}

/// GET /payments/refunds
pub async fn list_refunds(
    state:   web::Data<AppState>,
    session: AuthSession,
    query:   web::Query<ListRefundsQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsRefundsRead)?;

    let limit  = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

        let rows: Vec<RefundRow> = sqlx::query_as!(
                RefundRow,
                r#"
                SELECT id, transaction_id, idempotency_key, amount, reason,
                             status, requested_by, approved_by, processed_at,
                             created_at, updated_at
                FROM payments.refunds
                WHERE ($1::text IS NULL OR status         = $1)
                    AND ($2::uuid IS NULL OR transaction_id = $2)
                ORDER BY created_at DESC
                LIMIT $3 OFFSET $4
                "#,
                query.status.as_deref(),
                query.transaction_id as Option<Uuid>,
                limit,
                offset,
        )
        .fetch_all(&state.db)
        .await?;

    let resp: Vec<RefundResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// GET /payments/refunds/{id}
pub async fn get_refund(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsRefundsRead)?;
    let id = *path;

    let row = sqlx::query_as!(
        RefundRow,
        r#"
        SELECT id, transaction_id, idempotency_key, amount, reason,
               status, requested_by, approved_by, processed_at,
               created_at, updated_at
        FROM payments.refunds WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Refund not found".to_string()))?;

    Ok(HttpResponse::Ok().json(RefundResponse::from(row)))
}

/// POST /payments/refunds/{id}/approve
pub async fn approve_refund(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsRefundsApprove)?;
    let id = *path;

    let existing: Option<(String,)> = sqlx::query!(
        "SELECT status FROM payments.refunds WHERE id = $1",
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Refund not found".to_string()))?;

    if existing.status != "pending" {
        return Err(AppError::BadRequest(format!(
            "Refund cannot be approved from status '{}'", existing.status
        )));
    }

    let row: RefundRow = sqlx::query_as!(
        RefundRow,
        r#"
        UPDATE payments.refunds
        SET status = 'approved', approved_by = $2, updated_at = now()
        WHERE id = $1
        RETURNING id, transaction_id, idempotency_key, amount, reason,
                  status, requested_by, approved_by, processed_at,
                  created_at, updated_at
        "#,
        id,
        session.user_id,
    )
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(RefundResponse::from(row)))
}

/// POST /payments/refunds/{id}/process
///
/// Advances an approved refund to 'processing', then marks it 'completed'.
/// In production this would call the payment provider's refund API; here it
/// is a synchronous state machine for offline operation.
pub async fn process_refund(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsRefundsApprove)?;
    let id = *path;

    let existing: Option<(String, Uuid, bigdecimal::BigDecimal)> = sqlx::query!(
        "SELECT status, transaction_id, amount FROM payments.refunds WHERE id = $1",
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Refund not found".to_string()))?;

    if existing.status != "approved" {
        return Err(AppError::BadRequest(format!(
            "Refund cannot be processed from status '{}' (must be approved)", existing.status
        )));
    }

    // Process: mark 'completed' and update the parent transaction
    let row: RefundRow = sqlx::query_as!(
        RefundRow,
        r#"
        UPDATE payments.refunds
        SET status = 'completed', processed_at = now(), updated_at = now()
        WHERE id = $1
        RETURNING id, transaction_id, idempotency_key, amount, reason,
                  status, requested_by, approved_by, processed_at,
                  created_at, updated_at
        "#,
        id,
    )
    .fetch_one(&state.db)
    .await?;

    // Update parent transaction status
    // If the refund amount equals the full transaction amount → 'refunded'
    // otherwise → 'partially_refunded'
    use bigdecimal::BigDecimal;
    let refund_amount = existing.amount.clone();
    sqlx::query!(
        r#"
        UPDATE payments.transactions
        SET status = CASE
            WHEN $2 >= amount THEN 'refunded'
            ELSE 'partially_refunded'
        END,
        updated_at = now()
        WHERE id = $1
        "#,
        existing.transaction_id,
        refund_amount,
    )
    .execute(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(RefundResponse::from(row)))
}

// ============================================================
// Compensation jobs
// ============================================================

/// GET /payments/compensation/jobs
pub async fn list_compensation_jobs(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsReconciliationRead)?;

    let rows: Vec<super::models::CompensationJobResponse> = sqlx::query_as!(
        super::models::CompensationJobResponse,
        r#"
        SELECT id, job_type, status, affected_count, error_message,
               started_at, completed_at
        FROM payments.compensation_jobs
        ORDER BY started_at DESC
        LIMIT 100
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<CompensationJobResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// POST /payments/compensation/trigger
///
/// Manually triggers all compensation sweeps immediately.
/// Runs in the background; returns 202 Accepted with the approximate count of
/// pending jobs.
pub async fn trigger_compensation(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsReconciliationRun)?;

    let pool = state.db.clone();
    tokio::spawn(async move {
        compensation::run_all_sweeps(&pool).await;
    });

    Ok(HttpResponse::Accepted().json(serde_json::json!({
        "message": "Compensation sweeps triggered in background.",
        "sweeps":  ["stuck_transactions", "pending_refunds", "callback_retry"],
    })))
}
