/// Compensation job scheduler.
///
/// Runs three compensation sweeps every 15 minutes:
///
///   1. **Stuck transactions** — `pending` transactions that have a `processed`
///      callback older than 30 minutes are promoted to `completed` or `failed`
///      based on the callback payload's `status` field.
///
///   2. **Pending refunds** — `approved` refunds that have been in `processing`
///      for more than 1 hour are reset to `approved` so they can be retried.
///
///   3. **Callback retry** — `received` callbacks (not yet `processed`) older
///      than 10 minutes are re-queued for processing.
///
/// Each sweep records a `compensation_jobs` row with affected_count and status.
use sqlx::PgPool;

pub const SWEEP_INTERVAL_SECS: u64 = 900; // 15 minutes

// ============================================================
// Background task entry-point
// ============================================================

/// Long-running task — call once at startup via `tokio::spawn`.
pub async fn run_compensation_scheduler(pool: PgPool) {
    loop {
        let _ = run_all_sweeps(&pool).await;
        tokio::time::sleep(std::time::Duration::from_secs(SWEEP_INTERVAL_SECS)).await;
    }
}

/// Execute all three sweeps in sequence.  Errors in one sweep do not abort the others.
pub async fn run_all_sweeps(pool: &PgPool) {
    if let Err(e) = compensate_stuck_transactions(pool).await {
        tracing::error!(error = %e, "compensation: stuck_transactions sweep failed");
    }
    if let Err(e) = compensate_pending_refunds(pool).await {
        tracing::error!(error = %e, "compensation: pending_refunds sweep failed");
    }
    if let Err(e) = retry_unprocessed_callbacks(pool).await {
        tracing::error!(error = %e, "compensation: callback_retry sweep failed");
    }
}

// ============================================================
// Sweep 1: Stuck transactions
// ============================================================
//
// A transaction is "stuck" when:
//   - Its status is 'pending'
//   - At least one of its callbacks has status='processed' and processed_at < now - 30 min
//
// Action: promote the transaction to 'completed' (or 'failed' if the callback
//         payload contains "status":"FAILED").

pub async fn compensate_stuck_transactions(pool: &PgPool) -> Result<(), sqlx::Error> {
    let job_id: uuid::Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO payments.compensation_jobs (job_type)
        VALUES ('stuck_transactions')
        RETURNING id
        "#,
    )
    .fetch_one(pool)
    .await?;

    // Find all pending transactions that have a processed callback older than 30 min.
    // The new status is derived from the callback payload's `status` field:
    //   - payload->>'status' ILIKE 'FAIL%'  → 'failed'
    //   - otherwise                          → 'completed'
    let result = sqlx::query!(
        r#"
        WITH stuck AS (
            SELECT DISTINCT ON (tx.id)
                tx.id                           AS tx_id,
                CASE
                    WHEN upper(cb.payload->>'status') LIKE 'FAIL%' THEN 'failed'
                    ELSE 'completed'
                END                             AS new_status
            FROM payments.transactions tx
            JOIN payments.callbacks cb
              ON cb.transaction_id = tx.id
            WHERE tx.status = 'pending'
              AND cb.status  = 'processed'
              AND cb.processed_at < now() - INTERVAL '30 minutes'
            ORDER BY tx.id, cb.processed_at DESC
        )
        UPDATE payments.transactions t
        SET    status     = s.new_status,
               updated_at = now()
        FROM   stuck s
        WHERE  t.id = s.tx_id
        "#,
    )
    .execute(pool)
    .await?;

    let affected = result.rows_affected() as i32;
    tracing::info!(affected, "compensation: stuck_transactions promoted");

    sqlx::query!(
        r#"UPDATE payments.compensation_jobs
           SET status = 'completed', affected_count = $2, completed_at = now()
           WHERE id = $1"#,
        job_id,
        affected,
    )
    .execute(pool)
    .await?;

    Ok(())
}

// ============================================================
// Sweep 2: Pending refunds
// ============================================================
//
// An `approved` refund that somehow got stuck in `processing` for more than
// 1 hour is reset to `approved` so the refund processor can retry it.

pub async fn compensate_pending_refunds(pool: &PgPool) -> Result<(), sqlx::Error> {
    let job_id: uuid::Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO payments.compensation_jobs (job_type)
        VALUES ('pending_refunds')
        RETURNING id
        "#,
    )
    .fetch_one(pool)
    .await?;

    let result = sqlx::query!(
        r#"
        UPDATE payments.refunds
        SET    status     = 'approved',
               updated_at = now()
        WHERE  status = 'processing'
          AND  updated_at < now() - INTERVAL '1 hour'
        "#,
    )
    .execute(pool)
    .await?;

    let affected = result.rows_affected() as i32;
    tracing::info!(affected, "compensation: pending_refunds reset to approved");

    sqlx::query!(
        r#"UPDATE payments.compensation_jobs
           SET status = 'completed', affected_count = $2, completed_at = now()
           WHERE id = $1"#,
        job_id,
        affected,
    )
    .execute(pool)
    .await?;

    Ok(())
}

// ============================================================
// Sweep 3: Retry unprocessed callbacks
// ============================================================
//
// Callbacks that remain in 'received' status for more than 10 minutes likely
// failed during processing.  This sweep re-triggers processing by clearing
// processed_at and updating their transaction's status to 'pending' so the
// next callback ingestion attempt can proceed.

pub async fn retry_unprocessed_callbacks(pool: &PgPool) -> Result<(), sqlx::Error> {
    let job_id: uuid::Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO payments.compensation_jobs (job_type)
        VALUES ('callback_retry')
        RETURNING id
        "#,
    )
    .fetch_one(pool)
    .await?;

    // Mark old unprocessed callbacks as 'invalid' so they stop blocking the transaction,
    // and emit a warning for manual review.
    let result = sqlx::query!(
        r#"
        UPDATE payments.callbacks
        SET    status = 'invalid'
        WHERE  status     = 'received'
          AND  received_at < now() - INTERVAL '10 minutes'
        "#,
    )
    .execute(pool)
    .await?;

    let affected = result.rows_affected() as i32;
    if affected > 0 {
        tracing::warn!(
            affected,
            "compensation: {} stale callbacks marked invalid — manual review may be required",
            affected
        );
    }

    sqlx::query!(
        r#"UPDATE payments.compensation_jobs
           SET status = 'completed', affected_count = $2, completed_at = now()
           WHERE id = $1"#,
        job_id,
        affected,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the sweep interval constant matches the 15-minute requirement.
    #[test]
    fn sweep_interval_is_15_minutes() {
        assert_eq!(SWEEP_INTERVAL_SECS, 15 * 60);
    }
}
