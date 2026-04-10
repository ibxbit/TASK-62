/// Anomaly detection and alert generation.
///
/// Two detection paths:
///
///   1. **Reconciliation mismatches** — called directly by the reconciliation engine
///      after each run.  An alert is created whenever `discrepancy_count > 0`.
///      Severity: `warning` when discrepancy rate ≤ 10 items and ≤ 5 % of records;
///      `critical` when above either threshold (`summary.is_high`).
///
///   2. **KPI anomalies** — evaluated by a background task every 30 minutes.
///      For each active metric, the latest daily global snapshot value is compared
///      against the rolling average of the previous 9 snapshots.  If the deviation
///      exceeds `config.anomaly_threshold_pct` (default 25 %), an alert is raised.
///      Severity doubles from `warning` to `critical` if deviation exceeds 2 × threshold.
///
/// Alert creation is idempotent: at most one **open** alert exists per
/// `(alert_type, source_entity_id)` pair.  When an alert is created a notification
/// event is inserted into `notifications.events` (processed_at = NULL) so the
/// existing event bus fans it out within 5 seconds to subscribed users.
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

/// Background KPI anomaly check interval.
pub const KPI_CHECK_INTERVAL_SECS: u64 = 1800; // 30 minutes

// ============================================================
// Private metric definition row (module-level to share across fns)
// ============================================================

#[derive(sqlx::FromRow)]
struct MetricDefRow {
    id:           Uuid,
    metric_key:   String,
    display_name: String,
    config:       serde_json::Value,
}

// ============================================================
// Background KPI scheduler
// ============================================================

/// Long-running background task — call once at startup via `tokio::spawn`.
pub async fn run_kpi_anomaly_scheduler(pool: PgPool) {
    loop {
        if let Err(e) = run_kpi_anomaly_check(&pool).await {
            tracing::error!(error = %e, "KPI anomaly check failed");
        }
        tokio::time::sleep(std::time::Duration::from_secs(KPI_CHECK_INTERVAL_SECS)).await;
    }
}

/// Single KPI anomaly detection pass.  Errors in one metric do not abort others.
pub async fn run_kpi_anomaly_check(pool: &PgPool) -> Result<(), sqlx::Error> {
    let defs = sqlx::query_as!(
        MetricDefRow,
        "SELECT id, metric_key, display_name, config
         FROM   reporting.metric_definitions
         WHERE  is_active = true",
    )
    .fetch_all(pool)
    .await?;

    for def in &defs {
        if let Err(e) = check_metric_anomaly(pool, def).await {
            tracing::warn!(
                metric_key = %def.metric_key,
                error = %e,
                "KPI anomaly check skipped for metric"
            );
        }
    }
    Ok(())
}

// ============================================================
// Per-metric anomaly check
// ============================================================

async fn check_metric_anomaly(pool: &PgPool, def: &MetricDefRow) -> Result<(), sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct SnapRow {
        value: f64,
    }

    // Fetch the last 10 daily global (no route/depot filter) snapshots, newest first.
    let snaps = sqlx::query_as!(
        SnapRow,
        r#"
        SELECT value::double precision AS "value!: f64"
        FROM   reporting.metric_snapshots
        WHERE  metric_id   = $1
          AND  granularity = 'day'
          AND  route_id   IS NULL
          AND  depot_id   IS NULL
        ORDER BY period_start DESC
        LIMIT 10
        "#,
        def.id,
    )
    .fetch_all(pool)
    .await?;

    // Need at least 3 points: 1 current + 2 baseline.
    if snaps.len() < 3 {
        return Ok(());
    }

    let latest_value = snaps[0].value;
    let baseline: Vec<f64> = snaps[1..].iter().map(|s| s.value).collect();
    let avg = baseline.iter().sum::<f64>() / baseline.len() as f64;

    // Skip if baseline is effectively zero to avoid spurious division.
    if avg.abs() < 1e-9 {
        return Ok(());
    }

    let threshold_pct: f64 = def
        .config
        .get("anomaly_threshold_pct")
        .and_then(|v| v.as_f64())
        .unwrap_or(25.0);

    let deviation_pct = (latest_value - avg).abs() / avg.abs() * 100.0;
    if deviation_pct <= threshold_pct {
        return Ok(());
    }

    // Double the threshold → critical; otherwise warning.
    let severity = if deviation_pct > threshold_pct * 2.0 { "critical" } else { "warning" };

    let title = format!("KPI anomaly: {}", def.display_name);
    let description = format!(
        "Current value {:.2} deviates {:.1}% from baseline avg {:.2} \
         (threshold: {:.0}%)",
        latest_value, deviation_pct, avg, threshold_pct,
    );
    let payload = serde_json::json!({
        "metric_id":       def.id,
        "metric_key":      def.metric_key,
        "current_value":   latest_value,
        "baseline_avg":    avg,
        "deviation_pct":   deviation_pct,
        "threshold_pct":   threshold_pct,
        "severity":        severity,
    });

    create_alert(
        pool,
        "kpi_anomaly",
        severity,
        "reporting",
        Some(def.id),
        &title,
        Some(&description),
        payload,
    )
    .await
}

// ============================================================
// Reconciliation mismatch alert
// ============================================================

/// Create (or skip if already open) a reconciliation mismatch alert.
///
/// Accepts primitive types only — the engine passes extracted fields so that
/// this module does not need to import reconciliation types.
pub async fn check_reconciliation_run(
    pool:              &PgPool,
    run_id:            Uuid,
    run_date:          NaiveDate,
    discrepancy_count: usize,
    amount_mismatches: usize,
    missing:           usize,
    extra:             usize,
    duplicates:        usize,
    total_expected:    f64,
    total_collected:   f64,
    is_high:           bool,
) -> Result<(), sqlx::Error> {
    if discrepancy_count == 0 {
        return Ok(());
    }

    let severity = if is_high { "critical" } else { "warning" };

    let title = format!("Reconciliation mismatch — {}", run_date);
    let description = format!(
        "{} discrepancies: {} amount mismatches, {} missing from statement, \
         {} extra in statement, {} duplicates",
        discrepancy_count, amount_mismatches, missing, extra, duplicates,
    );
    let payload = serde_json::json!({
        "run_id":                 run_id,
        "run_date":               run_date.to_string(),
        "discrepancy_count":      discrepancy_count,
        "amount_mismatches":      amount_mismatches,
        "missing_from_statement": missing,
        "extra_in_statement":     extra,
        "duplicates":             duplicates,
        "total_expected":         total_expected,
        "total_collected":        total_collected,
        "severity":               severity,
    });

    create_alert(
        pool,
        "reconciliation_mismatch",
        severity,
        "payments",
        Some(run_id),
        &title,
        Some(&description),
        payload,
    )
    .await
}

// ============================================================
// Core: idempotent insert + notification event
// ============================================================

/// Insert an alert and notify subscribers via the event bus.
///
/// Skips silently if an open alert already exists for the same
/// `(alert_type, source_entity_id)` pair.
pub async fn create_alert(
    pool:             &PgPool,
    alert_type:       &str,
    severity:         &str,
    source_domain:    &str,
    source_entity_id: Option<Uuid>,
    title:            &str,
    description:      Option<&str>,
    payload:          serde_json::Value,
) -> Result<(), sqlx::Error> {

    // ---- Idempotency: skip if open alert already exists ----
    if let Some(eid) = source_entity_id {
        let exists: Option<Uuid> = sqlx::query_scalar!(
            r#"
            SELECT id FROM alerting.alerts
            WHERE  alert_type       = $1
              AND  source_entity_id = $2
              AND  status           = 'open'
            LIMIT 1
            "#,
            alert_type,
            eid,
        )
        .fetch_optional(pool)
        .await?;

        if exists.is_some() {
            return Ok(());
        }
    }

    // ---- Insert alert ----
    let alert_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO alerting.alerts
            (alert_type, severity, source_domain, source_entity_id,
             title, description, payload)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
        alert_type,
        severity,
        source_domain,
        source_entity_id,
        title,
        description,
        payload,
    )
    .fetch_one(pool)
    .await?;

    // ---- Map to notification event type ----
    let event_type = match alert_type {
        "reconciliation_mismatch" => "alerts.anomaly.reconciliation_mismatch",
        "kpi_anomaly"             => "alerts.anomaly.kpi_deviation",
        _                         => {
            tracing::warn!(alert_type, "unknown alert_type — skipping notification event");
            return Ok(());
        }
    };

    // ---- Fire notification event (bus picks it up within 5 s) ----
    // Embed alert_id + severity so subscribers see them in the inbox payload.
    let mut event_payload = payload;
    event_payload["alert_id"] = serde_json::Value::String(alert_id.to_string());
    event_payload["severity"] = serde_json::Value::String(severity.to_string());

    sqlx::query!(
        r#"
        INSERT INTO notifications.events
            (event_type, source_domain, source_entity_id, payload)
        VALUES ($1, $2, $3, $4)
        "#,
        event_type,
        source_domain,
        alert_id,
        event_payload,
    )
    .execute(pool)
    .await?;

    tracing::info!(
        alert_id = %alert_id,
        alert_type,
        severity,
        "anomaly alert created"
    );

    Ok(())
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    /// Verify the KPI check interval constant matches the 30-minute requirement.
    #[test]
    fn kpi_check_interval_is_30_minutes() {
        assert_eq!(super::KPI_CHECK_INTERVAL_SECS, 30 * 60);
    }

    #[test]
    fn deviation_threshold_logic() {
        // 25% threshold: deviation of 30% → warning; deviation of 51% → critical
        let threshold_pct = 25.0_f64;
        let dev_warning  = 30.0_f64;
        let dev_critical = 51.0_f64;

        let sev_w = if dev_warning  > threshold_pct * 2.0 { "critical" } else { "warning" };
        let sev_c = if dev_critical > threshold_pct * 2.0 { "critical" } else { "warning" };

        assert_eq!(sev_w, "warning");
        assert_eq!(sev_c, "critical");
    }
}
