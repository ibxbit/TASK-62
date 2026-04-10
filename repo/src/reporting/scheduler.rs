use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::models::{MetricQuery, ScheduledReportRow};
use super::metrics::compute_metric;

// ============================================================
// Background scheduler task
// ============================================================

/// Long-running background task that fires scheduled reports.
///
/// Wakes every 60 seconds and fetches all active scheduled_reports
/// whose `next_run_at <= now()`.  For each due report it:
///   1. Computes each metric in `metric_ids` over the configured rolling window.
///   2. Stores results in a `report_runs` row (status = completed / failed).
///   3. Advances `next_run_at` to the next occurrence.
pub async fn run_report_scheduler(pool: PgPool) {
    loop {
        if let Err(e) = tick(&pool).await {
            tracing::error!(error = %e, "report scheduler tick failed");
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

/// Run one scheduler tick: find all due scheduled reports and execute them.
///
/// Exposed as `pub` so the scheduler framework's `ReportGenerationJob` can
/// call it directly without going through the old bare loop.
pub async fn tick(pool: &PgPool) -> Result<(), sqlx::Error> {
    let due: Vec<ScheduledReportRow> = sqlx::query_as!(
        ScheduledReportRow,
        r#"
        SELECT id, name, metric_ids, schedule,
               route_id, depot_id, date_range_days,
               granularity, output_format,
               recipient_user_ids, is_active,
               next_run_at, last_run_at, created_by, created_at, updated_at
        FROM reporting.scheduled_reports
        WHERE is_active = TRUE
          AND next_run_at <= now()
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_all(pool)
    .await?;

    for report in due {
        process_scheduled_report(pool, &report).await;
    }

    Ok(())
}

/// Called from `handlers::trigger_run` when a user manually fires a scheduled report.
/// The run record is already created; this function computes the metrics and stores results.
pub async fn run_triggered_run(
    pool:     &PgPool,
    report:   &ScheduledReportRow,
    run_id:   Uuid,
    _user_id: Uuid,
) {
    let now       = Utc::now();
    let date_to   = now;
    let date_from = date_to - chrono::Duration::days(report.date_range_days as i64);

    let mut results = Vec::new();
    let mut failed  = false;

    for &metric_id in &report.metric_ids {
        let def = match sqlx::query_as!(
            super::models::MetricDefinitionRow,
            r#"
            SELECT id, metric_key, display_name, description,
                   formula_type, dimension_keys, config,
                   is_builtin, is_active, created_at, updated_at
            FROM reporting.metric_definitions
            WHERE id = $1 AND is_active = TRUE
            "#,
            metric_id,
        )
        .fetch_optional(pool)
        .await
        {
            Ok(Some(d)) => d,
            Ok(None) => {
                tracing::warn!(metric_id = %metric_id, "metric not found during triggered run");
                continue;
            }
            Err(e) => {
                tracing::error!(metric_id = %metric_id, error = %e, "metric load failed");
                failed = true;
                break;
            }
        };

        let query = MetricQuery {
            metric_id,
            date_from,
            date_to,
            granularity: report.granularity.clone(),
            route_id:    report.route_id,
            depot_id:    report.depot_id,
        };

        match compute_metric(pool, &def, &query).await {
            Ok(r)  => results.push(r),
            Err(e) => {
                tracing::error!(run_id = %run_id, error = %e, "metric computation failed");
                failed = true;
                break;
            }
        }
    }

    let result_json = serde_json::to_value(&results).unwrap_or(serde_json::Value::Null);

    if failed {
        let _ = sqlx::query!(
            "UPDATE reporting.report_runs SET status = 'failed', completed_at = now() WHERE id = $1",
            run_id
        )
        .execute(pool)
        .await;
    } else {
        let _ = sqlx::query!(
            r#"UPDATE reporting.report_runs
               SET status = 'completed', result_data = $2, completed_at = now()
               WHERE id = $1"#,
            run_id,
            result_json,
        )
        .execute(pool)
        .await;
    }
}

async fn process_scheduled_report(pool: &PgPool, report: &ScheduledReportRow) {
    let now     = Utc::now();
    let date_to = now;
    let date_from = date_to - Duration::days(report.date_range_days as i64);

    // Create a run record in 'running' state
    let run_id: Uuid = match sqlx::query_scalar!(
        r#"
        INSERT INTO reporting.report_runs
            (scheduled_id, metric_ids, route_id, depot_id,
             date_from, date_to, granularity, output_format, status, started_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'running', now())
        RETURNING id
        "#,
        report.id,
        &report.metric_ids,
        report.route_id,
        report.depot_id,
        date_from,
        date_to,
        report.granularity,
        report.output_format,
    )
    .fetch_one(pool)
    .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(scheduled_id = %report.id, error = %e, "failed to insert run record");
            return;
        }
    };

    // Compute each metric
    let mut results = Vec::new();
    let mut failed  = false;

    for &metric_id in &report.metric_ids {
        let def = match sqlx::query_as!(
            super::models::MetricDefinitionRow,
            r#"
            SELECT id, metric_key, display_name, description,
                   formula_type, dimension_keys, config,
                   is_builtin, is_active, created_at, updated_at
            FROM reporting.metric_definitions
            WHERE id = $1 AND is_active = TRUE
            "#,
            metric_id,
        )
        .fetch_optional(pool)
        .await
        {
            Ok(Some(d)) => d,
            Ok(None) => {
                tracing::warn!(metric_id = %metric_id, "metric not found or inactive during scheduled run");
                continue;
            }
            Err(e) => {
                tracing::error!(metric_id = %metric_id, error = %e, "failed to load metric def");
                failed = true;
                break;
            }
        };

        let query = MetricQuery {
            metric_id,
            date_from,
            date_to,
            granularity: report.granularity.clone(),
            route_id:    report.route_id,
            depot_id:    report.depot_id,
        };

        match compute_metric(pool, &def, &query).await {
            Ok(r)  => results.push(r),
            Err(e) => {
                tracing::error!(
                    metric_id = %metric_id,
                    scheduled_id = %report.id,
                    error = %e,
                    "metric computation failed"
                );
                failed = true;
                break;
            }
        }
    }

    // Persist results and advance schedule
    let result_json = serde_json::to_value(&results).unwrap_or(serde_json::Value::Null);

    if failed {
        let _ = sqlx::query!(
            r#"UPDATE reporting.report_runs
               SET status = 'failed', completed_at = now()
               WHERE id = $1"#,
            run_id
        )
        .execute(pool)
        .await;
    } else {
        let _ = sqlx::query!(
            r#"UPDATE reporting.report_runs
               SET status = 'completed', result_data = $2, completed_at = now()
               WHERE id = $1"#,
            run_id,
            result_json,
        )
        .execute(pool)
        .await;
    }

    // Advance `next_run_at` and update `last_run_at`
    let next = next_run_at(&report.schedule, now);
    let _ = sqlx::query!(
        r#"UPDATE reporting.scheduled_reports
           SET next_run_at = $2, last_run_at = now(), updated_at = now()
           WHERE id = $1"#,
        report.id,
        next,
    )
    .execute(pool)
    .await;
}

// ============================================================
// Schedule helpers
// ============================================================

/// Compute the next `next_run_at` timestamp for a given schedule string.
///
/// - `"daily"`   → `now + 1 day`
/// - `"weekly"`  → `now + 7 days`
/// - `"monthly"` → `now + 30 days` (simple approximation)
fn next_run_at(schedule: &str, from: chrono::DateTime<Utc>) -> chrono::DateTime<Utc> {
    match schedule {
        "weekly"  => from + Duration::days(7),
        "monthly" => from + Duration::days(30),
        _         => from + Duration::days(1),   // "daily" or unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn next_run_daily() {
        let base = Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();
        let next = next_run_at("daily", base);
        assert_eq!(next, base + Duration::days(1));
    }

    #[test]
    fn next_run_weekly() {
        let base = Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();
        let next = next_run_at("weekly", base);
        assert_eq!(next, base + Duration::days(7));
    }

    #[test]
    fn next_run_monthly() {
        let base = Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();
        let next = next_run_at("monthly", base);
        assert_eq!(next, base + Duration::days(30));
    }

    #[test]
    fn next_run_unknown_defaults_to_daily() {
        let base = Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();
        assert_eq!(next_run_at("unknown", base), next_run_at("daily", base));
    }
}
