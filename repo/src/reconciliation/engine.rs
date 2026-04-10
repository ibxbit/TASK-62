/// Reconciliation engine.
///
/// Given a set of statement records (validated by `importer`) and a target
/// `run_date`, the engine:
///
///   1. Marks duplicate statement entries.
///   2. Loads all `completed` DB transactions dated on `run_date`.
///   3. Matches each DB transaction to a statement record (primary key:
///      `idempotency_key = statement.ref`; fallback: `amount ≈ statement.amount`
///      on the same date).
///   4. Tags each pair with a `DiscrepancyType`.
///   5. Writes a `reconciliation_runs` row + all `reconciliation_items` rows.
///   6. Fires notification events via the `notifications.events` table so the
///      existing event bus fans them out to subscribed users.
use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use super::discrepancy::{
    canonical_index, classify_amounts, find_duplicates, is_duplicate, DiscrepancySummary,
    AMOUNT_TOLERANCE,
};
use super::models::{
    DiscrepancyType, ReconItem, ReconciliationOutput, StatementRecord,
};

// ============================================================
// DB transaction row (minimal projection)
// ============================================================

#[derive(sqlx::FromRow)]
struct TxnRow {
    id:               Uuid,
    idempotency_key:  String,
    /// Cast to float8 in SQL to avoid requiring the rust_decimal feature.
    amount:           f64,
}

// ============================================================
// Statement line ID row
// ============================================================

#[derive(sqlx::FromRow)]
struct LineRow {
    id:              Uuid,
    transaction_ref: Option<String>,
    /// Cast to float8 in SQL.
    amount:          f64,
}

// ============================================================
// Public entry-point
// ============================================================

/// Run full reconciliation for `run_date` using the statement records from
/// `statement_import_id`.
///
/// Returns a `ReconciliationOutput` describing every item; the caller (handler)
/// can use it to build the API response.
pub async fn run(
    pool:                &PgPool,
    run_date:             NaiveDate,
    statement_import_id:  Uuid,
    records:              &[StatementRecord],
    run_by:               Uuid,
) -> Result<ReconciliationOutput, sqlx::Error> {

    // ---- 1. Create / claim the reconciliation_runs row ----
    // If a run already exists for this date, use it (idempotent re-run).
    let run_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO payments.reconciliation_runs
            (run_date, status, statement_import_id, run_by, started_at)
        VALUES ($1, 'running', $2, $3, now())
        ON CONFLICT (run_date) DO UPDATE
            SET status               = 'running',
                statement_import_id  = EXCLUDED.statement_import_id,
                started_at           = now(),
                updated_at           = now()
        RETURNING id
        "#,
        run_date,
        statement_import_id,
        run_by,
    )
    .fetch_one(pool)
    .await?;

    // ---- 2. Detect duplicates in the statement ----
    let dup_map = find_duplicates(records);

    // Build a map: reference → first-occurrence index (canonical)
    // For duplicates, all non-canonical occurrences will be tagged 'duplicate'.
    let mut ref_to_canonical: HashMap<&str, usize> = HashMap::new();
    for (r, indices) in &dup_map {
        ref_to_canonical.insert(r, canonical_index(indices));
    }

    // ---- 3. Load DB transactions for run_date ----
    // Cast NUMERIC → float8 to avoid requiring the rust_decimal sqlx feature.
    let db_txns: Vec<TxnRow> = sqlx::query_as!(
        TxnRow,
        r#"
        SELECT id, idempotency_key, amount::double precision AS "amount!: f64"
        FROM   payments.transactions
        WHERE  status         = 'completed'
          AND  created_at::date = $1
        "#,
        run_date,
    )
    .fetch_all(pool)
    .await?;

    // ---- 4. Load statement_import_lines for this import ----
    let stmt_lines: Vec<LineRow> = sqlx::query_as!(
        LineRow,
        r#"
        SELECT id, transaction_ref, amount::double precision AS "amount!: f64"
        FROM   payments.statement_import_lines
        WHERE  import_id = $1
        "#,
        statement_import_id,
    )
    .fetch_all(pool)
    .await?;

    // Build a map: normalised_ref → line_id (only non-duplicate canonical lines)
    let mut ref_to_line: HashMap<String, Uuid> = HashMap::new();
    let mut ref_to_stmt_amount: HashMap<String, f64> = HashMap::new();
    for (idx, line) in stmt_lines.iter().enumerate() {
        if let Some(r) = &line.transaction_ref {
            let is_dup = dup_map.contains_key(r.as_str())
                && !ref_to_canonical
                    .get(r.as_str())
                    .map_or(false, |&ci| ci == idx);
            if !is_dup {
                ref_to_line.insert(r.clone(), line.id);
                ref_to_stmt_amount.insert(r.clone(), line.amount);
            }
        }
    }

    // ---- 5. Match DB transactions to statement records ----
    let mut items: Vec<ReconItem>        = Vec::new();
    let mut matched_stmt_refs: HashSet<String> = HashSet::new();
    let mut summary = DiscrepancySummary::default();

    for txn in &db_txns {
        let stmt_line_id   = ref_to_line.get(&txn.idempotency_key).copied();
        let stmt_amount    = ref_to_stmt_amount.get(&txn.idempotency_key).copied();

        let (disc, stmt_id, actual) = match (stmt_line_id, stmt_amount) {
            // Primary match by idempotency_key
            (Some(lid), Some(sa)) => {
                matched_stmt_refs.insert(txn.idempotency_key.clone());
                let dtype = classify_amounts(txn.amount, sa);
                (dtype, Some(lid), sa)
            }
            // Fallback: no ref match — look for amount+date match among unmatched lines
            _ => {
                let fallback = stmt_lines.iter().find(|l| {
                    l.transaction_ref.as_deref().map_or(true, |r| !matched_stmt_refs.contains(r))
                        && (l.amount - txn.amount).abs() <= AMOUNT_TOLERANCE
                });
                match fallback {
                    Some(fl) => {
                        if let Some(r) = &fl.transaction_ref {
                            matched_stmt_refs.insert(r.clone());
                        }
                        (DiscrepancyType::Matched, Some(fl.id), fl.amount)
                    }
                    None => {
                        // DB transaction not found in statement
                        (DiscrepancyType::MissingFromStatement, None, 0.0)
                    }
                }
            }
        };

        match disc {
            DiscrepancyType::Matched               => summary.matched += 1,
            DiscrepancyType::AmountMismatch        => summary.amount_mismatches += 1,
            DiscrepancyType::MissingFromStatement  => summary.missing_from_statement += 1,
            _ => {}
        }

        items.push(ReconItem {
            transaction_id:    Some(txn.id),
            statement_line_id: stmt_id,
            expected_amount:   txn.amount,
            actual_amount:     actual,
            discrepancy_type:  disc,
            notes:             None,
        });
    }

    // ---- 6. Find extra statement lines (no DB transaction) ----
    for line in &stmt_lines {
        let ref_matched = line
            .transaction_ref
            .as_deref()
            .map_or(false, |r| matched_stmt_refs.contains(r));

        if ref_matched { continue; }

        // Check if this is a duplicate line
        let is_dup = line
            .transaction_ref
            .as_deref()
            .map_or(false, |r| dup_map.contains_key(r));

        // Find the index of this line in the original records slice
        let line_idx = records
            .iter()
            .position(|r| r.reference == line.transaction_ref.as_deref().unwrap_or(""));

        let dtype = if is_dup && line_idx.map_or(false, |i| is_duplicate(i, &dup_map)) {
            summary.duplicates += 1;
            DiscrepancyType::Duplicate
        } else {
            summary.extra_in_statement += 1;
            DiscrepancyType::ExtraInStatement
        };

        items.push(ReconItem {
            transaction_id:    None,
            statement_line_id: Some(line.id),
            expected_amount:   0.0,
            actual_amount:     line.amount,
            discrepancy_type:  dtype,
            notes:             None,
        });
    }

    // ---- 7. Persist reconciliation_items ----
    let total_expected:  f64 = db_txns.iter().map(|t| t.amount).sum();
    let total_collected: f64 = stmt_lines.iter().map(|l| l.amount).sum();

    for item in &items {
        sqlx::query!(
            r#"
            INSERT INTO payments.reconciliation_items
                (run_id, transaction_id, expected_amount, actual_amount,
                 match_status, discrepancy_type, notes)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (run_id, transaction_id) DO UPDATE
                SET expected_amount  = EXCLUDED.expected_amount,
                    actual_amount    = EXCLUDED.actual_amount,
                    match_status     = EXCLUDED.match_status,
                    discrepancy_type = EXCLUDED.discrepancy_type,
                    notes            = EXCLUDED.notes
            "#,
            run_id,
            item.transaction_id,
            item.expected_amount,
            item.actual_amount,
            item.discrepancy_type.match_status(),
            item.discrepancy_type.as_str(),
            item.notes.as_deref(),
        )
        .execute(pool)
        .await?;
    }

    // ---- 8. Update reconciliation_runs summary ----
    let discrepancy_count = summary.total_discrepancies() as i32;

    sqlx::query!(
        r#"
        UPDATE payments.reconciliation_runs
        SET status            = 'completed',
            total_expected    = $2,
            total_collected   = $3,
            discrepancy_count = $4,
            completed_at      = now(),
            updated_at        = now()
        WHERE id = $1
        "#,
        run_id,
        total_expected,
        total_collected,
        discrepancy_count,
    )
    .execute(pool)
    .await?;

    // ---- 9. Fire notification events ----
    let total_records = items.len();
    emit_reconciliation_events(
        pool,
        run_id,
        run_date,
        &summary,
        total_records,
        total_expected,
        total_collected,
    )
    .await?;

    // ---- 10. Generate anomaly alert when discrepancies are present ----
    // Non-fatal: alert creation failure must not fail the reconciliation run.
    let disc = summary.total_discrepancies();
    if disc > 0 {
        let is_high = summary.is_high(total_records);
        crate::alerting::detector::check_reconciliation_run(
            pool,
            run_id,
            run_date,
            disc,
            summary.amount_mismatches,
            summary.missing_from_statement,
            summary.extra_in_statement,
            summary.duplicates,
            total_expected,
            total_collected,
            is_high,
        )
        .await
        .unwrap_or_else(|e| tracing::warn!(error = %e, "reconciliation alert creation failed"));
    }

    Ok(ReconciliationOutput {
        run_id,
        run_date,
        matched:            summary.matched,
        amount_mismatches:  summary.amount_mismatches,
        missing_from_stmt:  summary.missing_from_statement,
        extra_in_stmt:      summary.extra_in_statement,
        duplicates:         summary.duplicates,
        total_expected,
        total_collected,
        items,
    })
}

// ============================================================
// Notification event emission
// ============================================================

/// Insert notification events into `notifications.events` so the event bus
/// fans them out to subscribed users.
///
/// Events emitted:
///   - `payments.reconciliation.completed` (always)
///   - `payments.reconciliation.discrepancy_found` (when discrepancy_count > 0)
///   - `payments.reconciliation.high_discrepancy` (when is_high_discrepancy)
async fn emit_reconciliation_events(
    pool:              &PgPool,
    run_id:            Uuid,
    run_date:          NaiveDate,
    summary:           &DiscrepancySummary,
    total_records:     usize,
    total_expected:    f64,
    total_collected:   f64,
) -> Result<(), sqlx::Error> {
    let base_payload = serde_json::json!({
        "run_id":           run_id,
        "run_date":         run_date.to_string(),
        "matched":          summary.matched,
        "amount_mismatches": summary.amount_mismatches,
        "missing_from_statement": summary.missing_from_statement,
        "extra_in_statement": summary.extra_in_statement,
        "duplicates":       summary.duplicates,
        "discrepancy_count": summary.total_discrepancies(),
        "total_expected":   total_expected,
        "total_collected":  total_collected,
    });

    // Always: reconciliation completed
    let completed_severity = if summary.is_clean() { "info" } else { "warning" };
    let mut completed_payload = base_payload.clone();
    completed_payload["severity"] = serde_json::Value::String(completed_severity.to_string());

    sqlx::query!(
        r#"
        INSERT INTO notifications.events
            (event_type, source_domain, source_entity_id, payload)
        VALUES ('payments.reconciliation.completed', 'payments', $1, $2)
        "#,
        run_id,
        completed_payload,
    )
    .execute(pool)
    .await?;

    // When discrepancies found
    if !summary.is_clean() {
        sqlx::query!(
            r#"
            INSERT INTO notifications.events
                (event_type, source_domain, source_entity_id, payload)
            VALUES ('payments.reconciliation.discrepancy_found', 'payments', $1, $2)
            "#,
            run_id,
            base_payload,
        )
        .execute(pool)
        .await?;
    }

    // When high discrepancy volume
    if summary.is_high(total_records) {
        let mut critical_payload = base_payload.clone();
        critical_payload["severity"] = serde_json::Value::String("critical".to_string());

        sqlx::query!(
            r#"
            INSERT INTO notifications.events
                (event_type, source_domain, source_entity_id, payload)
            VALUES ('payments.reconciliation.high_discrepancy', 'payments', $1, $2)
            "#,
            run_id,
            critical_payload,
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::super::discrepancy::*;
    use super::super::models::*;
    use chrono::NaiveDate;

    fn rec(reference: &str, amount: f64) -> StatementRecord {
        StatementRecord {
            reference:   reference.to_string(),
            amount,
            entry_type:  EntryType::Credit,
            date:        NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
            description: None,
        }
    }

    #[test]
    fn matched_within_tolerance() {
        assert_eq!(classify_amounts(100.00, 100.005), DiscrepancyType::Matched);
    }

    #[test]
    fn mismatch_beyond_tolerance() {
        assert_eq!(classify_amounts(100.00, 100.02), DiscrepancyType::AmountMismatch);
    }

    #[test]
    fn duplicate_detection_in_engine_context() {
        let records = vec![
            rec("TXN-1", 50.0),
            rec("TXN-2", 75.0),
            rec("TXN-1", 50.0),  // duplicate
        ];
        let dups = find_duplicates(&records);
        assert!(dups.contains_key("TXN-1"));
        assert_eq!(dups["TXN-1"].len(), 2);
        assert_eq!(canonical_index(&dups["TXN-1"]), 0);
    }

    #[test]
    fn summary_totals_correctly() {
        let s = DiscrepancySummary {
            matched:               10,
            amount_mismatches:      2,
            missing_from_statement: 1,
            extra_in_statement:     1,
            duplicates:             0,
        };
        assert_eq!(s.total_discrepancies(), 4);
        assert!(!s.is_clean());
        assert!(!s.is_high(100));   // 4/100 = 4% ≤ 5%
    }
}
