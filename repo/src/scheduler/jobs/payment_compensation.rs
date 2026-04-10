use std::time::Duration;
use async_trait::async_trait;
use sqlx::PgPool;

use crate::payments::compensation;
use crate::scheduler::{Job, JobError, JobOutcome};

/// Runs the three payment compensation sweeps every 15 minutes.
///
/// Sweeps:
///   1. **stuck_transactions** — pending transactions with a processed callback
///      older than 30 min are promoted to `completed` or `failed`.
///   2. **pending_refunds** — approved refunds stuck in `processing` for > 1 hour
///      are reset to `approved` for retry.
///   3. **callback_retry** — `received` callbacks older than 10 min that were
///      never processed are re-queued.
///
/// Idempotency:
///   Each sweep selects `FOR UPDATE SKIP LOCKED` and uses status-guarded UPDATEs,
///   so re-running after a partial failure is safe.
pub struct PaymentCompensationJob;

#[async_trait]
impl Job for PaymentCompensationJob {
    fn name(&self) -> &'static str { "payment_compensation" }

    fn interval(&self) -> Duration {
        Duration::from_secs(compensation::SWEEP_INTERVAL_SECS)
    }

    async fn run(&self, pool: &PgPool) -> Result<JobOutcome, JobError> {
        compensation::run_all_sweeps(pool).await;
        Ok(JobOutcome {
            summary: serde_json::json!({ "sweeps": ["stuck_transactions", "pending_refunds", "callback_retry"] }),
        })
    }
}
