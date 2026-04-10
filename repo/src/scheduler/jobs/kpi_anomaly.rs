use std::time::Duration;
use async_trait::async_trait;
use sqlx::PgPool;

use crate::alerting::detector;
use crate::scheduler::{Job, JobError, JobOutcome};

/// Checks all active KPI metrics for anomalies every 30 minutes.
///
/// Per tick:
///   For each active `metric_definitions` row:
///     1. Fetch the latest daily snapshot value.
///     2. Compare against the rolling average of the previous 9 snapshots.
///     3. If deviation > `config.anomaly_threshold_pct` (default 25 %):
///        insert an `alerting.alerts` row (idempotent — at most one open alert
///        per `(alert_type, source_entity_id)`).
///        Also inserts a `notifications.events` row so the bus fans it out within 5 s.
///   Severity doubles from `warning` to `critical` when deviation > 2 × threshold.
///
/// Idempotency:
///   Alert creation is guarded by a partial unique index on `(alert_type,
///   source_entity_id)` WHERE `status = 'open'`, preventing duplicate open alerts.
pub struct KpiAnomalyJob;

#[async_trait]
impl Job for KpiAnomalyJob {
    fn name(&self) -> &'static str { "kpi_anomaly_check" }

    fn interval(&self) -> Duration {
        Duration::from_secs(detector::KPI_CHECK_INTERVAL_SECS)
    }

    async fn run(&self, pool: &PgPool) -> Result<JobOutcome, JobError> {
        detector::run_kpi_anomaly_check(pool).await?;
        Ok(JobOutcome {
            summary: serde_json::json!({ "action": "kpi_anomaly_check_completed" }),
        })
    }
}
