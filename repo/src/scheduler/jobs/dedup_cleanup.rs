use std::time::Duration;
use async_trait::async_trait;
use sqlx::PgPool;

use crate::scheduler::{Job, JobError, JobOutcome};

/// Data-retention sweep — runs every hour.
///
/// Maintains three tables to keep the notification deduplication window check
/// fast and prevent unbounded table growth:
///
/// | Table                                 | Retention | Condition                     |
/// |---------------------------------------|-----------|-------------------------------|
/// | `scheduler.job_runs`                  | 7 days    | status ∈ (success/failed/skipped) |
/// | `notifications.channel_deliveries`    | 30 days   | all rows                      |
/// | `notifications.deliveries` (dismissed)| 90 days   | status = 'dismissed'          |
/// | `notifications.events` (orphaned)     | 90 days   | processed + no live deliveries|
///
/// ## Deduplication window note
///
/// The bus deduplication check (`check_duplicate`) scans `notifications.deliveries`
/// for rows within the last 15 minutes.  As long as non-dismissed deliveries are
/// retained (inbox history), this check is correct.  Only dismissed rows older
/// than 90 days are removed — they are no longer in the dedup window by definition.
///
/// ## Idempotency
///
/// All statements are DELETE … WHERE with time-based predicates; re-running is safe.
pub struct DedupCleanupJob;

#[async_trait]
impl Job for DedupCleanupJob {
    fn name(&self) -> &'static str { "dedup_cleanup" }

    fn interval(&self) -> Duration { Duration::from_secs(3_600) } // 1 hour

    async fn run(&self, pool: &PgPool) -> Result<JobOutcome, JobError> {
        let job_runs         = prune_job_runs(pool).await?;
        let channel_delivs   = prune_channel_deliveries(pool).await?;
        let inbox_dismissed  = prune_dismissed_deliveries(pool).await?;
        let orphaned_events  = prune_orphaned_events(pool).await?;

        Ok(JobOutcome {
            summary: serde_json::json!({
                "job_runs_deleted":               job_runs,
                "channel_deliveries_deleted":     channel_delivs,
                "dismissed_deliveries_deleted":   inbox_dismissed,
                "orphaned_events_deleted":        orphaned_events,
            }),
        })
    }
}

// ── Sweep implementations ─────────────────────────────────────────────────────

/// Remove completed job_runs older than 7 days.
/// `running` records are intentionally excluded — they may be stale artefacts
/// that `Scheduler::recover_stale_runs` will repair on the next restart.
async fn prune_job_runs(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let n = sqlx::query(
        r#"
        DELETE FROM scheduler.job_runs
        WHERE  status     IN ('success', 'failed', 'skipped')
          AND  started_at  < now() - interval '7 days'
        "#,
    )
    .execute(pool)
    .await?
    .rows_affected();

    if n > 0 {
        tracing::info!(deleted = n, "dedup_cleanup: pruned old job_runs");
    }
    Ok(n)
}

/// Remove channel_deliveries older than 30 days.
/// These are external dispatch receipts; they are no longer needed for support
/// after 30 days (audit.audit_logs retains the source events for 7 years).
async fn prune_channel_deliveries(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let n = sqlx::query(
        r#"
        DELETE FROM notifications.channel_deliveries
        WHERE attempted_at < now() - interval '30 days'
        "#,
    )
    .execute(pool)
    .await?
    .rows_affected();

    if n > 0 {
        tracing::info!(deleted = n, "dedup_cleanup: pruned old channel_deliveries");
    }
    Ok(n)
}

/// Remove dismissed inbox deliveries older than 90 days.
/// Dismissed entries are outside the 15-minute dedup window by definition.
/// Their parent events may become orphaned and are collected by `prune_orphaned_events`.
async fn prune_dismissed_deliveries(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let n = sqlx::query(
        r#"
        DELETE FROM notifications.deliveries
        WHERE  status     = 'dismissed'
          AND  created_at < now() - interval '90 days'
        "#,
    )
    .execute(pool)
    .await?
    .rows_affected();

    if n > 0 {
        tracing::info!(deleted = n, "dedup_cleanup: pruned dismissed deliveries");
    }
    Ok(n)
}

/// Remove processed notification events that have no remaining deliveries.
///
/// This is safe because:
///   • Unprocessed events (`processed_at IS NULL`) are never deleted.
///   • Events with live (non-dismissed) deliveries are never deleted.
///   • The ON DELETE CASCADE on channel_deliveries.event_id ensures no
///     orphaned channel delivery records remain.
async fn prune_orphaned_events(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let n = sqlx::query(
        r#"
        DELETE FROM notifications.events e
        WHERE  e.processed_at IS NOT NULL
          AND  e.created_at   < now() - interval '90 days'
          AND  NOT EXISTS (
              SELECT 1 FROM notifications.deliveries d
              WHERE  d.event_id = e.id
          )
        "#,
    )
    .execute(pool)
    .await?
    .rows_affected();

    if n > 0 {
        tracing::info!(deleted = n, "dedup_cleanup: pruned orphaned notification events");
    }
    Ok(n)
}
