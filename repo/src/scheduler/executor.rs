/// Per-job execution engine: advisory lock → run → record outcome.
///
/// Advisory lock semantics
/// ───────────────────────
/// `pg_try_advisory_lock($1)` acquires a session-level exclusive lock identified
/// by a 64-bit integer.  It is:
///   • Non-blocking — returns false immediately if already held.
///   • Session-scoped — automatically released when the connection is closed
///     (i.e. when the process crashes), so no cleanup query is needed on restart.
///   • In-memory — no table write; negligible overhead even on 5-second intervals.
///
/// The lock ID is derived from the job name via FNV-1a, giving a stable mapping
/// that survives restarts without a lock table.
use std::sync::Arc;
use std::time::Instant;
use sqlx::PgPool;
use uuid::Uuid;

use super::{Job, JobError, ShutdownFlag};

// ── Main loop ─────────────────────────────────────────────────────────────────

/// Runs a job on its configured interval until `shutdown` is signalled.
/// Sleep is broken into 500 ms increments so shutdown is detected promptly.
pub async fn job_loop(pool: PgPool, job: Arc<dyn Job>, shutdown: ShutdownFlag) {
    tracing::info!(job = job.name(), interval_secs = job.interval().as_secs(), "Job loop started");

    loop {
        if shutdown.is_set() {
            break;
        }

        execute(&pool, &job).await;

        // Sleep in small slices so we react to shutdown within ~500 ms.
        let deadline = tokio::time::Instant::now() + job.interval();
        loop {
            if shutdown.is_set() {
                tracing::info!(job = job.name(), "Job loop exiting (shutdown)");
                return;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            tokio::time::sleep(remaining.min(std::time::Duration::from_millis(500))).await;
        }
    }

    tracing::info!(job = job.name(), "Job loop stopped");
}

// ── Single execution ──────────────────────────────────────────────────────────

async fn execute(pool: &PgPool, job: &Arc<dyn Job>) {
    let lock_id = advisory_lock_id(job.name());

    // Try to acquire — non-blocking
    let acquired: bool = match sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(lock_id)
        .fetch_one(pool)
        .await
    {
        Ok(b)  => b,
        Err(e) => {
            tracing::warn!(job = job.name(), error = %e, "Advisory lock query failed, skipping tick");
            return;
        }
    };

    if !acquired {
        tracing::debug!(job = job.name(), "Skipped — lock held by another instance");
        return;
    }

    let run_id = record_start(pool, job.name()).await;
    let t0     = Instant::now();

    let result = job.run(pool).await;
    let ms     = t0.elapsed().as_millis() as i32;

    // Release advisory lock before writing finish record so other instances
    // can proceed as quickly as possible.
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(lock_id)
        .execute(pool)
        .await;

    match run_id {
        Some(id) => record_finish(pool, id, ms, result).await,
        None => {
            // DB write of start record failed; still emit a log line.
            match result {
                Ok(o)  => tracing::info!(job = job.name(), ms, outcome = %o.summary, "Job succeeded (unrecorded)"),
                Err(e) => tracing::error!(job = job.name(), ms, error = %e, "Job failed (unrecorded)"),
            }
        }
    }
}

// ── DB helpers ────────────────────────────────────────────────────────────────

async fn record_start(pool: &PgPool, job_name: &str) -> Option<Uuid> {
    match sqlx::query_scalar(
        "INSERT INTO scheduler.job_runs (job_name, status) VALUES ($1, 'running') RETURNING id",
    )
    .bind(job_name)
    .fetch_one(pool)
    .await
    {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!(job = job_name, error = %e, "Failed to create job_run record");
            None
        }
    }
}

async fn record_finish(
    pool:   &PgPool,
    run_id: Uuid,
    ms:     i32,
    result: Result<super::JobOutcome, JobError>,
) {
    let (status, outcome, error_msg): (&str, Option<serde_json::Value>, Option<String>) =
        match result {
            Ok(o) => {
                tracing::info!(
                    run_id  = %run_id,
                    ms,
                    outcome = %o.summary,
                    "Job succeeded"
                );
                ("success", Some(o.summary), None)
            }
            Err(e) => {
                tracing::error!(run_id = %run_id, ms, error = %e, "Job failed");
                ("failed", None, Some(e.to_string()))
            }
        };

    let _ = sqlx::query(
        r#"UPDATE scheduler.job_runs
           SET    status      = $2,
                  finished_at = now(),
                  duration_ms = $3,
                  outcome     = $4,
                  error_msg   = $5
           WHERE  id = $1"#,
    )
    .bind(run_id)
    .bind(status)
    .bind(ms)
    .bind(outcome)
    .bind(error_msg)
    .execute(pool)
    .await
    .map_err(|e| tracing::warn!(run_id = %run_id, error = %e, "Failed to update job_run record"));
}

// ── Lock ID ───────────────────────────────────────────────────────────────────

/// Derive a stable i64 advisory lock identifier from a job name using FNV-1a.
/// The output is deterministic and collision-resistant for short ASCII strings.
pub fn advisory_lock_id(name: &str) -> i64 {
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    const PRIME:  u64 = 1_099_511_628_211;
    let mut h = OFFSET;
    for b in name.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    h as i64
}

#[cfg(test)]
mod tests {
    use super::advisory_lock_id;

    #[test]
    fn lock_ids_are_deterministic() {
        assert_eq!(advisory_lock_id("notification_bus"), advisory_lock_id("notification_bus"));
    }

    #[test]
    fn different_names_differ() {
        assert_ne!(
            advisory_lock_id("payment_compensation"),
            advisory_lock_id("report_generation")
        );
    }
}
