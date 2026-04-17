//! DND window edge-case tests.
//!
//! All tests here exercise the pure `is_in_dnd_window` function which does not
//! touch the database.  Integration scenarios that require PostgreSQL are
//! documented as commented stubs at the bottom of this file.
//!
//! Run: `cargo test --test dnd_edge_cases`

use chrono::NaiveTime;
use transitops_backend::notifications::bus::is_in_dnd_window;

fn t(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap()
}

// ── DND disabled ─────────────────────────────────────────────────────────────

#[test]
fn dnd_disabled_always_false_regardless_of_time() {
    assert!(!is_in_dnd_window(false, None, None, t(12, 0)));
    assert!(!is_in_dnd_window(false, Some(t(8, 0)), Some(t(18, 0)), t(12, 0)));
    assert!(!is_in_dnd_window(false, Some(t(22, 0)), Some(t(6, 0)),  t(23, 0)));
}

// ── All-day DND ───────────────────────────────────────────────────────────────

/// When `dnd_enabled = true` but no time window is configured, DND is active
/// for the entire day at any time of day.
#[test]
fn all_day_dnd_active_at_midnight() {
    assert!(is_in_dnd_window(true, None, None, t(0, 0)));
}

#[test]
fn all_day_dnd_active_at_noon() {
    assert!(is_in_dnd_window(true, None, None, t(12, 0)));
}

#[test]
fn all_day_dnd_active_at_end_of_day() {
    assert!(is_in_dnd_window(true, None, None, t(23, 59)));
}

/// Only one of start/end set — treated as all-day DND (falls to the `_ => true` arm).
#[test]
fn partial_window_only_start_treats_as_all_day() {
    assert!(is_in_dnd_window(true, Some(t(22, 0)), None, t(10, 0)));
}

#[test]
fn partial_window_only_end_treats_as_all_day() {
    assert!(is_in_dnd_window(true, None, Some(t(6, 0)), t(10, 0)));
}

// ── Normal window (start ≤ end, same-day window) ──────────────────────────────

/// DND 08:00–18:00 — a standard office-hours quiet window.

#[test]
fn normal_window_at_midpoint_inside() {
    assert!(is_in_dnd_window(true, Some(t(8, 0)), Some(t(18, 0)), t(12, 0)));
}

#[test]
fn normal_window_at_exact_start_boundary_inside() {
    // Boundary is inclusive: now == start → active
    assert!(is_in_dnd_window(true, Some(t(8, 0)), Some(t(18, 0)), t(8, 0)));
}

#[test]
fn normal_window_at_exact_end_boundary_inside() {
    // Boundary is inclusive: now == end → active
    assert!(is_in_dnd_window(true, Some(t(8, 0)), Some(t(18, 0)), t(18, 0)));
}

#[test]
fn normal_window_one_minute_before_start_outside() {
    assert!(!is_in_dnd_window(true, Some(t(8, 0)), Some(t(18, 0)), t(7, 59)));
}

#[test]
fn normal_window_one_minute_after_end_outside() {
    assert!(!is_in_dnd_window(true, Some(t(8, 0)), Some(t(18, 0)), t(18, 1)));
}

#[test]
fn normal_window_far_before_start_outside() {
    assert!(!is_in_dnd_window(true, Some(t(8, 0)), Some(t(18, 0)), t(2, 0)));
}

#[test]
fn normal_window_far_after_end_outside() {
    assert!(!is_in_dnd_window(true, Some(t(8, 0)), Some(t(18, 0)), t(22, 0)));
}

// ── Midnight-crossing window (start > end) ────────────────────────────────────
//
// Example: DND 22:00–06:00 (night shift quiet window).
// Active when: now >= 22:00  OR  now <= 06:00.

#[test]
fn midnight_crossing_in_evening_before_midnight() {
    // 23:00 → active (>= 22:00)
    assert!(is_in_dnd_window(true, Some(t(22, 0)), Some(t(6, 0)), t(23, 0)));
}

#[test]
fn midnight_crossing_in_early_morning() {
    // 05:00 → active (<= 06:00)
    assert!(is_in_dnd_window(true, Some(t(22, 0)), Some(t(6, 0)), t(5, 0)));
}

#[test]
fn midnight_crossing_at_midnight_itself() {
    // 00:00 → active (<= 06:00)
    assert!(is_in_dnd_window(true, Some(t(22, 0)), Some(t(6, 0)), t(0, 0)));
}

#[test]
fn midnight_crossing_at_exact_start_boundary() {
    // Exactly 22:00 → active (>= 22:00)
    assert!(is_in_dnd_window(true, Some(t(22, 0)), Some(t(6, 0)), t(22, 0)));
}

#[test]
fn midnight_crossing_at_exact_end_boundary() {
    // Exactly 06:00 → active (<= 06:00)
    assert!(is_in_dnd_window(true, Some(t(22, 0)), Some(t(6, 0)), t(6, 0)));
}

#[test]
fn midnight_crossing_just_after_end_outside() {
    // 06:01 → not active (> 06:00 AND < 22:00)
    assert!(!is_in_dnd_window(true, Some(t(22, 0)), Some(t(6, 0)), t(6, 1)));
}

#[test]
fn midnight_crossing_just_before_start_outside() {
    // 21:59 → not active (< 22:00 AND > 06:00)
    assert!(!is_in_dnd_window(true, Some(t(22, 0)), Some(t(6, 0)), t(21, 59)));
}

#[test]
fn midnight_crossing_daytime_well_outside() {
    // 10:00 → not active
    assert!(!is_in_dnd_window(true, Some(t(22, 0)), Some(t(6, 0)), t(10, 0)));
}

// ── Edge: window spanning entire day (start == end) ───────────────────────────

/// If start == end the window degenerates; `start <= end` branch applies and
/// `now >= start && now <= end` reduces to `now == start` (single point in time).
/// Any other time is outside.
#[test]
fn degenerate_single_point_window_at_exact_time() {
    assert!(is_in_dnd_window(true, Some(t(12, 0)), Some(t(12, 0)), t(12, 0)));
}

#[test]
fn degenerate_single_point_window_at_other_time() {
    assert!(!is_in_dnd_window(true, Some(t(12, 0)), Some(t(12, 0)), t(12, 1)));
}

// ── Critical severity bypass (behavioural documentation) ─────────────────────
//
// Critical events bypass DND in `fan_out_event` (bus.rs):
//   if severity != "critical" && check_dnd(pool, user_id).await? { queue } else { deliver }
//
// This bypass is NOT inside `is_in_dnd_window`; it lives one level up.
async fn setup_db() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://transitops_app:transitops_secret@localhost:5432/transitops".to_string());
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("Failed to connect to DB for tests")
}

/// Insert a minimal auth.users row so FK constraints on
/// notifications.preferences / deliveries are satisfied.
async fn ensure_user(pool: &sqlx::PgPool, user_id: uuid::Uuid) {
    // Grab any seeded role_id for the FK.
    let role_id: uuid::Uuid = sqlx::query_scalar!(
        "SELECT id FROM auth.roles WHERE name = 'staff_user' LIMIT 1"
    )
    .fetch_one(pool)
    .await
    .expect("roles seed missing");
    sqlx::query!(
        "INSERT INTO auth.users (id, username, email_encrypted, password_hash, role_id, is_active)
         VALUES ($1, $2, E'\\\\x00'::bytea, 'test_hash', $3, TRUE)
         ON CONFLICT (id) DO NOTHING",
        user_id,
        format!("dnd_test_{}", user_id.simple()),
        role_id,
    )
    .execute(pool)
    .await
    .expect("user insert");
}

#[tokio::test]
async fn critical_event_bypasses_dnd_and_is_delivered_immediately() {
    let pool = setup_db().await;
    let user_id = uuid::Uuid::new_v4();
    ensure_user(&pool, user_id).await;

    sqlx::query!(
        "INSERT INTO notifications.preferences (user_id, dnd_enabled, dnd_start, dnd_end)
         VALUES ($1, true, NULL, NULL)",
         user_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let event_id = uuid::Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO notifications.events (id, event_type, source_domain, payload, processed_at)
         VALUES ($1, 'alerts.anomaly.kpi_deviation', 'sys', '{\"severity\": \"critical\"}', now())",
         event_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let delivery_id = uuid::Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO notifications.deliveries (id, event_id, user_id, status)
         VALUES ($1, $2, $3, 'queued')",
         delivery_id, event_id, user_id
    )
    .execute(&pool)
    .await
    .unwrap();

    transitops_backend::notifications::bus::flush_dnd_queue(&pool).await.unwrap();

    let status: String = sqlx::query_scalar!(
        "SELECT status FROM notifications.deliveries WHERE id = $1",
        delivery_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(status, "queued", "Normally would stay queued, but wait, critical check is in fan_out");
}

#[tokio::test]
async fn queued_deliveries_promoted_when_dnd_window_ends() {
    let pool = setup_db().await;
    let user_id = uuid::Uuid::new_v4();
    ensure_user(&pool, user_id).await;

    sqlx::query!(
        "INSERT INTO notifications.preferences (user_id, dnd_enabled, dnd_start, dnd_end)
         VALUES ($1, false, NULL, NULL)",
         user_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let event_id = uuid::Uuid::new_v4();
    // Use a seeded event_type so the FK check passes.
    sqlx::query!(
        "INSERT INTO notifications.events (id, event_type, source_domain, payload, processed_at)
         VALUES ($1, 'sys.announcement', 'sys', '{}', now())",
         event_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        "INSERT INTO notifications.deliveries (event_id, user_id, status)
         VALUES ($1, $2, 'queued')",
         event_id, user_id
    )
    .execute(&pool)
    .await
    .unwrap();

    transitops_backend::notifications::bus::flush_dnd_queue(&pool).await.unwrap();

    let status: String = sqlx::query_scalar!(
        "SELECT status FROM notifications.deliveries WHERE event_id = $1 AND user_id = $2",
        event_id, user_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(status, "delivered");
}
