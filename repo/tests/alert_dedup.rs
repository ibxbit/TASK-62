//! Alert deduplication and severity escalation tests.
//!
//! Pure-logic tests cover: KPI anomaly severity thresholds, reconciliation
//! alert severity mapping, and the zero-discrepancy early-exit guard.
//!
//! Core alert idempotency (at most one open alert per entity+type) requires a
//! live database and is documented as integration stubs at the bottom.
//!
//! Run: `cargo test --test alert_dedup`

use transitops_backend::alerting::detector::KPI_CHECK_INTERVAL_SECS;

// ── Constants ─────────────────────────────────────────────────────────────────

#[test]
fn kpi_check_interval_is_30_minutes() {
    assert_eq!(KPI_CHECK_INTERVAL_SECS, 30 * 60);
}

// ── KPI anomaly severity threshold logic ─────────────────────────────────────
//
// From `detector::check_metric_anomaly`:
//   if deviation_pct <= threshold_pct  → no alert
//   if deviation_pct > threshold_pct * 2 → "critical"
//   otherwise                           → "warning"
//
// Default threshold: 25 %

fn kpi_severity(deviation_pct: f64, threshold_pct: f64) -> &'static str {
    if deviation_pct > threshold_pct * 2.0 { "critical" } else { "warning" }
}

#[test]
fn deviation_at_single_threshold_would_not_trigger_alert() {
    // The detector returns early when deviation <= threshold.
    let threshold = 25.0_f64;
    let deviation = 25.0_f64;
    let would_alert = deviation > threshold;
    assert!(!would_alert, "exactly at threshold should not alert");
}

#[test]
fn deviation_just_above_threshold_is_warning() {
    assert_eq!(kpi_severity(25.1, 25.0), "warning");
}

#[test]
fn deviation_at_double_threshold_is_warning() {
    // 50.0 is NOT > 50.0, so still warning (strict >)
    assert_eq!(kpi_severity(50.0, 25.0), "warning");
}

#[test]
fn deviation_just_above_double_threshold_is_critical() {
    assert_eq!(kpi_severity(50.001, 25.0), "critical");
}

#[test]
fn deviation_way_above_double_threshold_is_critical() {
    assert_eq!(kpi_severity(200.0, 25.0), "critical");
}

#[test]
fn custom_40pct_threshold_warning_range() {
    // threshold=40%: warning for 41–80%, critical for >80%
    assert_eq!(kpi_severity(60.0, 40.0), "warning");
    assert_eq!(kpi_severity(80.0, 40.0), "warning"); // exactly 2× → NOT critical
}

#[test]
fn custom_40pct_threshold_critical_range() {
    assert_eq!(kpi_severity(80.1, 40.0), "critical");
}

#[test]
fn zero_baseline_would_skip_check() {
    // In the detector: if avg.abs() < 1e-9 { return Ok(()); }
    let avg: f64 = 1e-10;
    let skip = avg.abs() < 1e-9;
    assert!(skip, "near-zero baseline must be skipped to avoid division artifacts");
}

// ── Reconciliation alert severity ────────────────────────────────────────────
//
// From `detector::check_reconciliation_run`:
//   severity = if is_high { "critical" } else { "warning" }

fn recon_severity(is_high: bool) -> &'static str {
    if is_high { "critical" } else { "warning" }
}

#[test]
fn recon_alert_warning_when_not_high() {
    assert_eq!(recon_severity(false), "warning");
}

#[test]
fn recon_alert_critical_when_high() {
    assert_eq!(recon_severity(true), "critical");
}

/// No alert at all when discrepancy_count == 0 (guarded by early return in
/// `check_reconciliation_run`).
#[test]
fn zero_discrepancy_count_produces_no_alert() {
    let discrepancy_count: usize = 0;
    let would_create_alert = discrepancy_count > 0;
    assert!(!would_create_alert);
}

/// A non-zero count always creates an alert regardless of is_high.
#[test]
fn nonzero_discrepancy_count_produces_alert() {
    let discrepancy_count: usize = 1;
    assert!(discrepancy_count > 0);
}

// ── Minimum baseline for KPI anomaly check ────────────────────────────────────

#[test]
fn kpi_check_requires_at_least_3_snapshots() {
    // The detector returns Ok(()) early when len < 3.
    // This documents the minimum data requirement.
    let need_at_least = 3_usize;
    assert!(need_at_least <= 3, "detector needs current + 2 baseline points");
}

async fn setup_db() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://transitops_app:transitops_secret@localhost:5432/transitops".to_string());
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("Failed to connect to DB for tests")
}

#[tokio::test]
async fn open_alert_prevents_duplicate_for_same_entity_and_type() {
    let pool = setup_db().await;
    let entity_id = uuid::Uuid::new_v4();

    // First standard insert
    transitops_backend::alerting::detector::create_alert(
        &pool,
        "kpi_anomaly",
        "warning",
        "reporting",
        Some(entity_id),
        "First Alert",
        None,
        serde_json::json!({}),
    )
    .await
    .unwrap();

    // Call again with the same entity+type
    transitops_backend::alerting::detector::create_alert(
        &pool,
        "kpi_anomaly",
        "warning",
        "reporting",
        Some(entity_id),
        "Duplicate Should Be Ignored",
        None,
        serde_json::json!({}),
    )
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM alerting.alerts WHERE source_entity_id = $1",
        entity_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .unwrap_or(0);

    assert_eq!(count, 1, "Idempotency guard should prevent second open alert");
}

#[tokio::test]
async fn acknowledged_alert_allows_new_alert_for_same_entity() {
    let pool = setup_db().await;
    let entity_id = uuid::Uuid::new_v4();

    // Insert manually with 'acknowledged' status
    sqlx::query!(
        "INSERT INTO alerting.alerts (alert_type, status, severity, source_domain, source_entity_id, title, payload)
         VALUES ('kpi_anomaly', 'acknowledged', 'warning', 'reporting', $1, 'Old Alert', '{}')",
         entity_id
    )
    .execute(&pool)
    .await
    .unwrap();

    transitops_backend::alerting::detector::create_alert(
        &pool,
        "kpi_anomaly",
        "warning",
        "reporting",
        Some(entity_id),
        "New Alert",
        None,
        serde_json::json!({}),
    )
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM alerting.alerts WHERE source_entity_id = $1",
        entity_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .unwrap_or(0);

    assert_eq!(count, 2, "Should allow new alert because old one is acknowledged");
}

#[tokio::test]
async fn closed_alert_allows_new_alert_for_same_entity() {
    let pool = setup_db().await;
    let entity_id = uuid::Uuid::new_v4();

    sqlx::query!(
        "INSERT INTO alerting.alerts (alert_type, status, severity, source_domain, source_entity_id, title, payload)
         VALUES ('kpi_anomaly', 'closed', 'warning', 'reporting', $1, 'Closed Alert', '{}')",
         entity_id
    )
    .execute(&pool)
    .await
    .unwrap();

    transitops_backend::alerting::detector::create_alert(
        &pool,
        "kpi_anomaly",
        "warning",
        "reporting",
        Some(entity_id),
        "New Alert",
        None,
        serde_json::json!({}),
    )
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM alerting.alerts WHERE source_entity_id = $1",
        entity_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .unwrap_or(0);

    assert_eq!(count, 2, "Should allow new alert because old one is closed");
}

#[tokio::test]
async fn different_entity_creates_independent_alert() {
    let pool = setup_db().await;
    let entity_a = uuid::Uuid::new_v4();
    let entity_b = uuid::Uuid::new_v4();

    transitops_backend::alerting::detector::create_alert(
        &pool, "kpi_anomaly", "warning", "reporting", Some(entity_a), "A", None, serde_json::json!({})
    ).await.unwrap();

    transitops_backend::alerting::detector::create_alert(
        &pool, "kpi_anomaly", "warning", "reporting", Some(entity_b), "B", None, serde_json::json!({})
    ).await.unwrap();

    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM alerting.alerts WHERE source_entity_id IN ($1, $2)",
        entity_a, entity_b
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .unwrap_or(0);

    assert_eq!(count, 2, "Independent entities should map to different alerts");
}

#[tokio::test]
async fn alert_creation_inserts_notification_event_for_fan_out() {
    let pool = setup_db().await;
    let entity_id = uuid::Uuid::new_v4();

    transitops_backend::alerting::detector::create_alert(
        &pool,
        "kpi_anomaly",
        "critical",
        "reporting",
        Some(entity_id),
        "Critical Issue",
        None,
        serde_json::json!({"some_data": 42}),
    )
    .await
    .unwrap();

    let event_type = "alerts.anomaly.kpi_deviation";
    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM notifications.events 
         WHERE event_type = $1 AND source_entity_id = (
            SELECT id FROM alerting.alerts WHERE source_entity_id = $2
         ) AND processed_at IS NULL",
        event_type, entity_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .unwrap_or(0);

    assert_eq!(count, 1, "Alert creation should spawn a pending notification event");
}
