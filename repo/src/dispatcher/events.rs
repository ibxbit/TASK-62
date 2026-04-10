/// Thin wrapper around `notifications.events` INSERT.
///
/// Every dispatcher action that changes observable state emits an event here.
/// The event type must exist in `notifications.event_definitions` (see seed 003).
/// Failures are non-fatal: logged but never bubbled up to the caller so a
/// transactional data write is never blocked by a notification insert.
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

// ── Event type constants ─────────────────────────────────────────────────────
pub const TRIP_MODIFIED:          &str = "ops.trip.modified";
pub const TRIP_DRIVER_ASSIGNED:   &str = "ops.trip.driver_assigned";
pub const TRIP_STARTED:           &str = "ops.trip.started";
pub const TRIP_COMPLETED:         &str = "ops.trip.completed";
pub const TRIP_CANCELLED:         &str = "ops.trip.cancelled";
pub const TRIP_CONFLICT_DETECTED: &str = "ops.trip.conflict_detected";
pub const TRIP_START_APPROACHING: &str = "ops.trip.start_approaching";

/// Insert a domain event into `notifications.events`.
///
/// * `entity_id`  — the primary subject of the event (usually the trip UUID)
/// * `actor_id`   — the user who triggered it; `None` for system-generated events
/// * `payload`    — free-form JSON context carried with the event
pub async fn emit(
    pool:      &PgPool,
    event_type: &str,
    entity_id:  Option<Uuid>,
    actor_id:   Option<Uuid>,
    payload:    Value,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO notifications.events
            (event_type, source_domain, source_entity_id, actor_id, payload)
        VALUES ($1, 'ops', $2, $3, $4)
        "#,
    )
    .bind(event_type)
    .bind(entity_id)
    .bind(actor_id)
    .bind(payload)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(event_type, ?entity_id, "Failed to emit event: {}", e);
    }
}
