use std::time::Duration;
use async_trait::async_trait;
use sqlx::PgPool;

use crate::notifications::{adapters::AdapterRegistry, bus};
use crate::scheduler::{Job, JobError, JobOutcome};

/// Processes pending notification events and flushes the DND queue.
///
/// Interval: 5 seconds.
///
/// Per tick:
///   1. Fan-out up to 50 unprocessed events to subscribed users.
///   2. Evaluate keyword/topic/threshold rules across the batch.
///   3. Promote `queued` deliveries for users whose DND window has ended.
///
/// Idempotency:
///   `process_pending_events` selects `WHERE processed_at IS NULL LIMIT 50` —
///   leaving the row unprocessed on error, so the next tick retries it.
///   `flush_dnd_queue` uses a single UPDATE and is naturally idempotent.
pub struct NotificationBusJob {
    pub adapters: AdapterRegistry,
}

#[async_trait]
impl Job for NotificationBusJob {
    fn name(&self) -> &'static str { "notification_bus" }

    fn interval(&self) -> Duration { Duration::from_secs(5) }

    /// Lock timeout is short because this job is expected to finish in < 1 s.
    fn lock_timeout(&self) -> Duration { Duration::from_secs(30) }

    async fn run(&self, pool: &PgPool) -> Result<JobOutcome, JobError> {
        let (processed, flushed) = bus::tick_once(pool, &self.adapters).await?;
        Ok(JobOutcome {
            summary: serde_json::json!({
                "events_processed":     processed,
                "deliveries_flushed":   flushed,
            }),
        })
    }
}
