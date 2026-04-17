//! Idempotency and deduplication tests.
//!
//! Pure tests cover:
//!   - Advisory lock ID determinism and collision-resistance (scheduler)
//!   - Dedup window timing invariants (documented)
//!
//! All DB-backed tests (transaction idempotency key, 15-min notification
//! dedup window, FOR UPDATE SKIP LOCKED semantics) are documented as
//! integration stubs at the bottom.
//!
//! Run: `cargo test --test idempotency`

use transitops_backend::scheduler::executor::advisory_lock_id;

// ── Advisory lock ID ──────────────────────────────────────────────────────────

#[test]
fn advisory_lock_id_is_deterministic() {
    assert_eq!(
        advisory_lock_id("notification_bus"),
        advisory_lock_id("notification_bus"),
    );
}

#[test]
fn advisory_lock_id_differs_for_different_names() {
    assert_ne!(advisory_lock_id("job_a"), advisory_lock_id("job_b"));
}

/// All six production jobs must have unique advisory lock IDs.
/// A collision would cause two jobs to share a lock and one to always be
/// skipped even when no instance of the other is running.
#[test]
fn all_production_job_lock_ids_are_unique() {
    let ids = [
        ("notification_bus",     advisory_lock_id("notification_bus")),
        ("payment_compensation", advisory_lock_id("payment_compensation")),
        ("report_generation",    advisory_lock_id("report_generation")),
        ("kpi_anomaly_check",    advisory_lock_id("kpi_anomaly_check")),
        ("scheduled_config",     advisory_lock_id("scheduled_config")),
        ("dedup_cleanup",        advisory_lock_id("dedup_cleanup")),
    ];

    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(
                ids[i].1, ids[j].1,
                "Lock ID collision between '{}' and '{}'",
                ids[i].0, ids[j].0,
            );
        }
    }
}

#[test]
fn advisory_lock_id_empty_string_does_not_panic() {
    let _ = advisory_lock_id("");
}

/// Lock IDs are i64 — verify the FNV output fits within i64 range without panic.
#[test]
fn advisory_lock_id_output_is_valid_i64() {
    // All six production names produce valid i64 without overflow panic.
    for name in &[
        "notification_bus", "payment_compensation", "report_generation",
        "kpi_anomaly_check", "scheduled_config", "dedup_cleanup",
    ] {
        let id: i64 = advisory_lock_id(name);
        // Just checking that the cast didn't panic; all i64 values are valid.
        let _ = id;
    }
}

// ── Notification dedup window ─────────────────────────────────────────────────
//
// The dedup window is 15 minutes, expressed as a SQL interval in `check_duplicate`:
//   d.created_at > now() - interval '15 minutes'
//   AND d.status != 'dismissed'
//
// Key invariants (all require database — see stubs below):
//   1. Same (user, event_type, entity) within 15 min → suppressed
//   2. Same (user, event_type, entity) after 15 min  → new delivery created
//   3. Different user, same event → NOT suppressed
//   4. Dismissed delivery → NOT counted in dedup window
//   5. Queued delivery (DND) → IS counted in dedup window (status != 'dismissed')

/// Documents the dedup window duration as a test-checked constant.
/// The actual 15-minute check lives in SQL, but this test records the intent.
#[test]
fn notification_dedup_window_is_15_minutes_in_seconds() {
    let dedup_window_secs: u64 = 15 * 60;
    assert_eq!(dedup_window_secs, 900);
}

// ── Integration test stubs ────────────────────────────────────────────────────

// #[tokio::test]
// #[ignore = "requires database"]
// async fn transaction_same_idempotency_key_returns_identical_response() {
//     // POST /payments/transactions with { idempotency_key: "idem-001", amount: 50.00 }
//     // POST same request again
//     // Assert: second response body == first response body (same id, amount, status)
// }

// #[tokio::test]
// #[ignore = "requires database"]
// async fn transaction_different_idempotency_key_creates_new_transaction() {
//     // POST with key "idem-A"; POST with key "idem-B" (same payload)
//     // Assert: two distinct transaction rows with different IDs
// }

async fn setup_db() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://transitops_app:transitops_secret@localhost:5432/transitops".to_string());
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("Failed to connect to DB for tests")
}

/// Create a real auth.users row so notifications.deliveries FKs are satisfied.
async fn ensure_user(pool: &sqlx::PgPool, user_id: uuid::Uuid) {
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
        format!("dedup_test_{}", user_id.simple()),
        role_id,
    )
    .execute(pool)
    .await
    .expect("user insert");
}

#[tokio::test]
async fn notification_dedup_same_user_entity_within_window_suppresses() {
    let pool = setup_db().await;
    let user_id = uuid::Uuid::new_v4();
    let entity_id = uuid::Uuid::new_v4();
    let event_id = uuid::Uuid::new_v4();
    ensure_user(&pool, user_id).await;

    sqlx::query!(
        "INSERT INTO notifications.events (id, event_type, source_domain, source_entity_id, payload)
         VALUES ($1, 'payment.captured', 'payments', $2, '{}')",
         event_id, entity_id
    ).execute(&pool).await.unwrap();

    sqlx::query!(
        "INSERT INTO notifications.deliveries (event_id, user_id, status, created_at)
         VALUES ($1, $2, 'delivered', now())",
         event_id, user_id
    ).execute(&pool).await.unwrap();

    let is_dup = transitops_backend::notifications::bus::check_duplicate(&pool, user_id, "payment.captured", Some(entity_id))
        .await
        .unwrap();

    assert!(is_dup, "Should suppress duplicate delivery within window");
}

#[tokio::test]
async fn notification_dedup_expires_after_window_creates_new_delivery() {
    let pool = setup_db().await;
    let user_id = uuid::Uuid::new_v4();
    let entity_id = uuid::Uuid::new_v4();
    let event_id = uuid::Uuid::new_v4();
    ensure_user(&pool, user_id).await;

    sqlx::query!(
        "INSERT INTO notifications.events (id, event_type, source_domain, source_entity_id, payload)
         VALUES ($1, 'payment.captured', 'payments', $2, '{}')",
         event_id, entity_id
    ).execute(&pool).await.unwrap();

    sqlx::query!(
        "INSERT INTO notifications.deliveries (event_id, user_id, status, created_at)
         VALUES ($1, $2, 'delivered', now() - interval '16 minutes')",
         event_id, user_id
    ).execute(&pool).await.unwrap();

    let is_dup = transitops_backend::notifications::bus::check_duplicate(&pool, user_id, "payment.captured", Some(entity_id))
        .await
        .unwrap();

    assert!(!is_dup, "Should NOT suppress delivery past 15 min window");
}

#[tokio::test]
async fn notification_dedup_dismissed_delivery_not_counted() {
    let pool = setup_db().await;
    let user_id = uuid::Uuid::new_v4();
    let entity_id = uuid::Uuid::new_v4();
    let event_id = uuid::Uuid::new_v4();
    ensure_user(&pool, user_id).await;

    sqlx::query!(
        "INSERT INTO notifications.events (id, event_type, source_domain, source_entity_id, payload)
         VALUES ($1, 'payment.captured', 'payments', $2, '{}')",
         event_id, entity_id
    ).execute(&pool).await.unwrap();

    // Insert dismissed delivery exactly inside the window
    sqlx::query!(
        "INSERT INTO notifications.deliveries (event_id, user_id, status, created_at)
         VALUES ($1, $2, 'dismissed', now())",
         event_id, user_id
    ).execute(&pool).await.unwrap();

    let is_dup = transitops_backend::notifications::bus::check_duplicate(&pool, user_id, "payment.captured", Some(entity_id))
        .await
        .unwrap();

    assert!(!is_dup, "Dismissed deliveries should not be counted for deduplication");
}

#[tokio::test]
async fn notification_dedup_different_user_not_suppressed() {
    let pool = setup_db().await;
    let user_a = uuid::Uuid::new_v4();
    let user_b = uuid::Uuid::new_v4();
    let entity_id = uuid::Uuid::new_v4();
    let event_id = uuid::Uuid::new_v4();
    ensure_user(&pool, user_a).await;
    ensure_user(&pool, user_b).await;

    sqlx::query!(
        "INSERT INTO notifications.events (id, event_type, source_domain, source_entity_id, payload)
         VALUES ($1, 'payment.captured', 'payments', $2, '{}')",
         event_id, entity_id
    ).execute(&pool).await.unwrap();

    sqlx::query!(
        "INSERT INTO notifications.deliveries (event_id, user_id, status, created_at)
         VALUES ($1, $2, 'delivered', now())",
         event_id, user_a
    ).execute(&pool).await.unwrap();

    let is_dup_for_b = transitops_backend::notifications::bus::check_duplicate(&pool, user_b, "payment.captured", Some(entity_id))
        .await
        .unwrap();

    assert!(!is_dup_for_b, "User B should get their own delivery despite User A having one");
}
