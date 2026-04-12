use actix_web::{web, HttpResponse};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chrono::Utc;
use uuid::Uuid;

use bigdecimal::BigDecimal;
use bigdecimal::Zero;

use crate::auth::middleware::{AuthSession, ReauthGuard};
use crate::error::AppError;
use crate::rbac::permissions::Permission;
use crate::AppState;

use super::engine;
use bigdecimal::ToPrimitive;
use super::importer;
use super::models::{
    ItemResponse, ListItemsQuery, ListRunsQuery, ReconciliationItemRow, ReconciliationRunRow,
    RunResponse, StartRunRequest, UploadResponse,
};
use super::discrepancy::AMOUNT_TOLERANCE;

// ============================================================
// Statement upload
// ============================================================

/// POST /reconciliation/statements
///
/// Upload a daily bank/provider statement CSV for a given date.
///
/// Body (JSON):
/// ```json
/// {
///   "filename":            "alipay_2025-01-15.csv",
///   "source":              "alipay",
///   "content_base64":      "<base64-encoded CSV>",
///   "expected_fingerprint": "<sha256-hex>"   // optional
/// }
/// ```
///
/// Steps:
///   1. Decode base64 content.
///   2. Compute SHA-256 fingerprint.
///   3. If `expected_fingerprint` provided, verify it matches (reject on mismatch).
///   4. Detect duplicate file (same hash previously imported).
///   5. Validate CSV format strictly.
///   6. Store in `statement_imports` with validation metadata.
///
/// Returns 400 if the fingerprint doesn't match or the file was already imported.
/// Returns the import record regardless of format errors — callers can inspect
/// `errors` and decide whether to trigger reconciliation.
#[derive(serde::Deserialize)]
pub struct UploadStatementRequest {
    pub filename:             String,
    pub source:               String,
    pub content_base64:       String,
    pub expected_fingerprint: Option<String>,
}

pub async fn upload_statement(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<UploadStatementRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsStatementsImport)?;

    // ---- Decode ----
    let content = B64.decode(&body.content_base64).map_err(|e| {
        AppError::BadRequest(format!("content_base64 is not valid base64: {}", e))
    })?;

    // ---- Fingerprint ----
    let actual_fp = importer::fingerprint(&content);

    if let Some(ref expected) = body.expected_fingerprint {
        if !importer::verify_fingerprint(&content, expected) {
            return Err(AppError::BadRequest(format!(
                "Fingerprint mismatch — expected {}, got {}",
                expected, actual_fp
            )));
        }
    }

    // ---- Duplicate file check ----
    let existing_id: Option<Uuid> = sqlx::query_scalar!(
        "SELECT id FROM payments.statement_imports WHERE file_hash = $1",
        actual_fp,
    )
    .fetch_optional(&state.db)
    .await?;

    if let Some(eid) = existing_id {
        return Err(AppError::BadRequest(format!(
            "File already imported — existing import_id: {}", eid
        )));
    }

    // ---- Format validation ----
    let parse = importer::validate_and_parse(&content);

    let format_errors_json = serde_json::to_value(&parse.errors).unwrap_or(serde_json::json!([]));

    // ---- Persist import record ----
    let import_date       = Utc::now().date_naive();
    let encrypted_content = state.crypto.encrypt_bytes(&content)?;
    let import_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO payments.statement_imports
            (filename, file_hash, source, import_date, imported_by,
             raw_content_encrypted, format_errors, fingerprint_expected)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id
        "#,
        body.filename,
        actual_fp,
        body.source,
        import_date,
        session.user_id,
        encrypted_content.as_slice(),
        format_errors_json,
        body.expected_fingerprint.as_deref(),
    )
    .fetch_one(&state.db)
    .await?;

    // ---- Persist parsed lines (only when valid) ----
    if parse.is_valid {
        for (line_no, rec) in parse.records.iter().enumerate() {
            sqlx::query!(
                r#"
                INSERT INTO payments.statement_import_lines
                    (import_id, line_number, transaction_ref, amount,
                     transaction_date, description, match_status)
                VALUES ($1, $2, $3, $4, $5, $6, 'unmatched')
                "#,
                import_id,
                (line_no + 1) as i32,
                rec.reference.as_str(),
                rec.amount,
                rec.date,
                rec.description.as_deref(),
            )
            .execute(&state.db)
            .await?;
        }

        // Mark duplicates
        tag_duplicate_lines(&state.db, import_id, &parse.records).await?;
    }

    // Update status
    let status = if parse.is_valid { "completed" } else { "failed" };
    sqlx::query!(
        r#"UPDATE payments.statement_imports
           SET status = $2, total_records = $3, processed_records = $3, updated_at = now()
           WHERE id = $1"#,
        import_id,
        status,
        parse.records.len() as i32,
    )
    .execute(&state.db)
    .await?;

    Ok(HttpResponse::Created().json(UploadResponse {
        import_id,
        fingerprint:  actual_fp,
        record_count: parse.records.len(),
        is_valid:     parse.is_valid,
        errors:       parse.errors,
    }))
}

/// Tag duplicate lines in `statement_import_lines` after initial insert.
async fn tag_duplicate_lines(
    pool:      &sqlx::PgPool,
    import_id: Uuid,
    records:   &[super::models::StatementRecord],
) -> Result<(), AppError> {
    use super::discrepancy::{canonical_index, find_duplicates};

    let dup_map = find_duplicates(records);
    if dup_map.is_empty() { return Ok(()); }

    // For each duplicate group: look up line IDs by line_number, then set
    // duplicate_of_line_id on non-canonical rows.
    for (_ref, indices) in &dup_map {
        let canonical_line_no = (canonical_index(indices) + 1) as i32;

        // Get the canonical line's ID
        let canonical_id: Option<Uuid> = sqlx::query_scalar!(
            "SELECT id as \"id!\" FROM payments.statement_import_lines WHERE import_id = $1 AND line_number = $2",
            import_id,
            canonical_line_no,
        )
        .fetch_optional(pool)
        .await
        .map_err(AppError::Database)?;

        let Some(cid) = canonical_id else { continue };

        // Tag all non-canonical lines in this group
        for &idx in indices {
            let line_no = (idx + 1) as i32;
            if line_no == canonical_line_no { continue; }

            let _result: sqlx::postgres::PgQueryResult = sqlx::query!(
                r#"UPDATE payments.statement_import_lines
                   SET duplicate_of_line_id = $3
                   WHERE import_id = $1 AND line_number = $2"#,
                import_id,
                line_no,
                cid,
            )
            .execute(pool)
            .await
            .map_err(AppError::Database)?;
        }
    }

    Ok(())
}

// ============================================================
// Reconciliation runs
// ============================================================

/// POST /reconciliation/runs
///
/// Start a reconciliation run against a previously uploaded statement.
/// Idempotent: if a run already exists for `run_date`, it is re-executed
/// and its results overwritten.
/// Requires re-authentication within the last 10 minutes.
pub async fn start_run(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    body:    web::Json<StartRunRequest>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::PaymentsReconciliationRun)?;

    // Validate run_date is present now that RBAC + reauth checks have passed.
    let run_date = body.run_date.ok_or_else(|| {
        AppError::BadRequest("run_date is required".to_string())
    })?;

    // ---- Verify the import exists and is in a usable state ----
    let import = sqlx::query!(
        "SELECT status, file_hash, raw_content_encrypted FROM payments.statement_imports WHERE id = $1",
        body.statement_import_id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Statement import not found".to_string()))?;

    let status = &import.status;

    if status == "failed" {
        return Err(AppError::BadRequest(
            "Cannot reconcile: the statement import failed format validation".to_string(),
        ));
    }

    // Verify file integrity
    if let Some(expected) = body.expected_fingerprint.as_ref() {
        if &import.file_hash != expected {
            return Err(AppError::BadRequest("Statement hash mismatch".to_string()));
        }
    }

    // Decrypt if necessary
    let raw_encrypted = import.raw_content_encrypted
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("No statement content to process".to_string()))?;

    let raw_content = state.crypto.decrypt_bytes(raw_encrypted)
        .map_err(|_| AppError::Internal("Failed to decrypt statement".to_string()))?;

    let parse = importer::validate_and_parse(&raw_content);
    if !parse.is_valid {
        return Err(AppError::BadRequest(format!(
            "Statement has {} format error(s): {}",
            parse.errors.len(),
            parse.errors.join("; ")
        )));
    }

    // ---- Run the engine ----
    let output = engine::run(
        &state.db,
        run_date,
        body.statement_import_id,
        &parse.records,
        session.user_id,
    )
    .await?;

    crate::audit::writer::log(
        &state.db,
        &session,
        crate::audit::writer::AuditEntry {
            action:       crate::audit::writer::ACTION_RECON_RUN,
            domain:       "payments",
            entity_type:  "reconciliation_run",
            entity_id:    output.run_id.to_string(),
            before_state: None,
            after_state:  None,
            metadata:     serde_json::json!({
                "run_date":          output.run_date,
                "discrepancy_count": output.discrepancy_count(),
                "total_expected":    output.total_expected,
                "total_collected":   output.total_collected,
            }),
        },
    )
    .await
    .unwrap_or_else(|e| tracing::warn!(error = %e, "audit log failed"));

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "run_id":              output.run_id,
        "run_date":            output.run_date,
        "matched":             output.matched,
        "amount_mismatches":   output.amount_mismatches,
        "missing_from_statement": output.missing_from_stmt,
        "extra_in_statement":  output.extra_in_stmt,
        "duplicates":          output.duplicates,
        "discrepancy_count":   output.discrepancy_count(),
        "total_expected":      output.total_expected.clone(),
        "total_collected":     output.total_collected.clone(),
        "total_discrepancy":   output.total_collected.to_f64().unwrap_or(0.0) - output.total_expected.to_f64().unwrap_or(0.0),
        "is_high_discrepancy": output.is_high_discrepancy(output.items.len()),
    })))
}

/// GET /reconciliation/runs
pub async fn list_runs(
    state:   web::Data<AppState>,
    session: AuthSession,
    query:   web::Query<ListRunsQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsReconciliationRead)?;

    let limit  = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    let rows: Vec<ReconciliationRunRow> = sqlx::query_as!(
         ReconciliationRunRow,
         r#"
         SELECT id, run_date, status, statement_import_id,
             total_expected::numeric as "total_expected!", total_collected::numeric as "total_collected!", discrepancy_count,
             started_at, completed_at, run_by, notes,
             created_at, updated_at
         FROM payments.reconciliation_runs
         WHERE ($1::text IS NULL OR status = $1)
         ORDER BY run_date DESC
         LIMIT $2 OFFSET $3
         "#,
         query.status.as_deref(),
         limit,
         offset,
        )
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<RunResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// GET /reconciliation/runs/{id}
pub async fn get_run(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsReconciliationRead)?;
    let id = *path;

    let row: ReconciliationRunRow = sqlx::query_as!(
         ReconciliationRunRow,
         r#"
         SELECT id, run_date, status, statement_import_id,
             total_expected::numeric as "total_expected!", total_collected::numeric as "total_collected!", discrepancy_count,
             started_at, completed_at, run_by, notes,
             created_at, updated_at
         FROM payments.reconciliation_runs WHERE id = $1
         "#,
         id,
        )
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Reconciliation run not found".to_string()))?;

    Ok(HttpResponse::Ok().json(RunResponse::from(row)))
}

/// GET /reconciliation/runs/{id}/items
///
/// Returns all reconciliation items for a run, optionally filtered by
/// `discrepancy_type`.  Useful for building a discrepancy report.
pub async fn list_items(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    query:   web::Query<ListItemsQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsReconciliationRead)?;
    let run_id = *path;

    let limit  = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);

    let rows: Vec<ReconciliationItemRow> = sqlx::query_as!(
                ReconciliationItemRow,
                r#"
                SELECT id, run_id, transaction_id,
                             expected_amount::numeric as "expected_amount!", actual_amount::numeric as "actual_amount!",
                             match_status, discrepancy_type, notes, created_at
                FROM payments.reconciliation_items
                WHERE run_id = $1
                    AND ($2::text IS NULL OR discrepancy_type = $2)
                ORDER BY created_at
                LIMIT $3 OFFSET $4
                "#,
                run_id,
                query.discrepancy_type.as_deref(),
                limit,
                offset,
        )
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<ItemResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// GET /reconciliation/runs/{id}/summary
///
/// Aggregated discrepancy summary for a run — useful for dashboard widgets.
pub async fn run_summary(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsReconciliationRead)?;
    let run_id = *path;

    // Verify run exists
    let run = sqlx::query!(
        r#"
        SELECT total_expected::double precision as "total_expected!", 
               total_collected::double precision as "total_collected!", 
               discrepancy_count
        FROM payments.reconciliation_runs WHERE id = $1
        "#,
        run_id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Reconciliation run not found".to_string()))?;

    // Count by discrepancy_type
    #[derive(sqlx::FromRow)]
    struct CountRow { discrepancy_type: Option<String>, cnt: Option<i64> }

    let counts: Vec<CountRow> = sqlx::query_as!(
        CountRow,
        r#"
        SELECT discrepancy_type, COUNT(*) AS cnt
        FROM payments.reconciliation_items
        WHERE run_id = $1
        GROUP BY discrepancy_type
        "#,
        run_id,
    )
    .fetch_all(&state.db)
    .await?;

    let mut by_type: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    for c in &counts {
        if let Some(ref t) = c.discrepancy_type {
            by_type.insert(t.clone(), serde_json::json!(c.cnt.unwrap_or(0)));
        }
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "run_id":            run_id,
        "total_expected":    run.total_expected,
        "total_collected":   run.total_collected,
        "total_discrepancy": run.total_collected - run.total_expected,
        "discrepancy_count": run.discrepancy_count,
        "amount_tolerance":  AMOUNT_TOLERANCE,
        "by_type":           by_type,
    })))
}

/// GET /reconciliation/statements
pub async fn list_statements(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::PaymentsStatementsRead)?;

    #[derive(sqlx::FromRow, serde::Serialize)]
    struct StmtRow {
        id:              Uuid,
        filename:        String,
        file_hash:       String,
        source:          String,
        import_date:     chrono::NaiveDate,
        status:          String,
        total_records:   i32,
        error_count:     i32,
        imported_by:     Uuid,
        created_at:      chrono::DateTime<Utc>,
    }

    let rows: Vec<StmtRow> = sqlx::query_as!(
        StmtRow,
        r#"
        SELECT id, filename, file_hash, source, import_date, status,
               total_records, error_count, imported_by, created_at
        FROM payments.statement_imports
        ORDER BY import_date DESC, created_at DESC
        LIMIT 100
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(rows))
}
