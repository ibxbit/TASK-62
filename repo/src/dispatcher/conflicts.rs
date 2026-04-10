/// Conflict detection engine.
///
/// Three conflict classes are detected:
///
///  ┌──────────────────────────┬──────────────┬──────────────────────────────────┐
///  │ Type                     │ Severity     │ Trigger                          │
///  ├──────────────────────────┼──────────────┼──────────────────────────────────┤
///  │ driver_overlap           │ critical     │ same driver assigned to two       │
///  │                          │              │ time-overlapping trips             │
///  ├──────────────────────────┼──────────────┼──────────────────────────────────┤
///  │ route_headway            │ critical /   │ two trips on the same route depart│
///  │                          │ warning      │ < 1 min (critical) or < 5 min     │
///  │                          │              │ (warning) apart                   │
///  ├──────────────────────────┼──────────────┼──────────────────────────────────┤
///  │ unassigned_approaching   │ critical /   │ trip is starting soon with no     │
///  │                          │ warning      │ driver: < 10 min (critical),      │
///  │                          │              │ < 30 min (warning)                │
///  └──────────────────────────┴──────────────┴──────────────────────────────────┘
///
/// All functions return `Vec<ConflictDetail>`.  Callers are responsible for
/// persisting via `save_conflicts` and emitting events.
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::dispatcher::models::ConflictRow;

// ── Thresholds ───────────────────────────────────────────────────────────────
pub const HEADWAY_CRITICAL_SECS: i64 = 60;  // < 1 min  → critical
pub const HEADWAY_WARNING_SECS:  i64 = 300; // 1–5 min  → warning
pub const APPROACHING_CRITICAL_MINS: i64 = 10;
pub const APPROACHING_WARNING_MINS:  i64 = 30;

// ── Input type ───────────────────────────────────────────────────────────────

/// Snapshot of a trip used as input to conflict detection.
/// Callers build this from a DB row after creating or updating a trip.
pub struct TripCheckInput {
    pub trip_id:              Uuid,
    pub trip_code:            String,
    pub route_id:             Uuid,
    pub assigned_driver_id:   Option<Uuid>,
    pub scheduled_departure:  DateTime<Utc>,
    pub scheduled_arrival:    DateTime<Utc>,
}

/// A single detected conflict, not yet persisted.
pub struct ConflictDetail {
    pub conflict_type:   &'static str,
    pub trip_id_1:       Uuid,
    pub trip_id_1_code:  String,
    pub trip_id_2:       Option<Uuid>,
    pub trip_id_2_code:  Option<String>,
    pub description:     String,
    pub severity:        &'static str,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Run all conflict checks for `trip` and return a list of detected conflicts.
/// Does NOT write to DB or emit events.
pub async fn detect_conflicts(
    pool: &PgPool,
    trip: &TripCheckInput,
) -> Result<Vec<ConflictDetail>, sqlx::Error> {
    let mut all = Vec::new();

    if let Some(driver_id) = trip.assigned_driver_id {
        all.extend(detect_driver_overlap(pool, trip, driver_id).await?);
    }
    all.extend(detect_route_headway(pool, trip).await?);

    Ok(all)
}

/// Persist `conflicts`, skipping any that already have an open record for the
/// same (conflict_type, trip_id_1, trip_id_2) triple.
/// Returns the UUIDs of newly inserted rows.
pub async fn save_conflicts(
    pool:      &PgPool,
    conflicts: &[ConflictDetail],
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut new_ids = Vec::new();

    for c in conflicts {
        // Deduplication: skip if an open conflict of this type already exists
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM ops.trip_conflicts
                WHERE  conflict_type = $1
                  AND  trip_id_1     = $2
                  AND  (
                       ($3::UUID IS NULL     AND trip_id_2 IS NULL)
                    OR (trip_id_2 = $3::UUID)
                  )
                  AND  status = 'open'
            )
            "#,
        )
        .bind(c.conflict_type)
        .bind(c.trip_id_1)
        .bind(c.trip_id_2)
        .fetch_one(pool)
        .await?;

        if !exists {
            let id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO ops.trip_conflicts
                    (conflict_type, trip_id_1, trip_id_2, description, severity)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id
                "#,
            )
            .bind(c.conflict_type)
            .bind(c.trip_id_1)
            .bind(c.trip_id_2)
            .bind(&c.description)
            .bind(c.severity)
            .fetch_one(pool)
            .await?;

            new_ids.push(id);
        }
    }

    Ok(new_ids)
}

/// Detect + save in one call.  Returns UUIDs of newly persisted conflicts.
pub async fn check_and_save(
    pool: &PgPool,
    trip: &TripCheckInput,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let detected = detect_conflicts(pool, trip).await?;
    save_conflicts(pool, &detected).await
}

/// Detect unassigned-approaching conflicts across ALL upcoming trips.
/// Called by the scheduler endpoint `POST /dispatcher/monitor/check-approaching`.
/// Returns a list of `(trip_id, severity)` pairs for newly created conflicts.
pub async fn detect_approaching_unassigned(
    pool: &PgPool,
) -> Result<Vec<(Uuid, &'static str)>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct ApproachingRow {
        id:                   Uuid,
        trip_code:            String,
        scheduled_departure:  DateTime<Utc>,
        assigned_driver_id:   Option<Uuid>,
    }

    // Trips starting within the warning window with no driver assigned
    let rows = sqlx::query_as::<_, ApproachingRow>(
        r#"
        SELECT id, trip_code, scheduled_departure, assigned_driver_id
        FROM   ops.trips
        WHERE  deleted_at           IS NULL
          AND  assigned_driver_id   IS NULL
          AND  status               IN ('published', 'draft', 'scheduled')
          AND  scheduled_departure  BETWEEN now()
                               AND  now() + ($1 * interval '1 minute')
        ORDER  BY scheduled_departure ASC
        "#,
    )
    .bind(APPROACHING_WARNING_MINS)
    .fetch_all(pool)
    .await?;

    let mut results = Vec::new();

    for row in rows {
        let mins_away = (row.scheduled_departure - Utc::now())
            .num_minutes()
            .max(0);

        let severity: &'static str = if mins_away < APPROACHING_CRITICAL_MINS {
            "critical"
        } else {
            "warning"
        };

        let desc = format!(
            "Trip {} has no assigned driver and departs in {} minute(s) ({} severity)",
            row.trip_code, mins_away, severity
        );

        let detail = ConflictDetail {
            conflict_type:  "unassigned_approaching",
            trip_id_1:      row.id,
            trip_id_1_code: row.trip_code.clone(),
            trip_id_2:      None,
            trip_id_2_code: None,
            description:    desc,
            severity,
        };

        let new_ids = save_conflicts(pool, &[detail]).await?;
        if !new_ids.is_empty() {
            results.push((row.id, severity));
        }
    }

    Ok(results)
}

/// Load open conflicts for a trip (as both trip_id_1 and trip_id_2) with trip codes.
pub async fn fetch_conflicts_for_trip(
    pool:    &PgPool,
    trip_id: Uuid,
) -> Result<Vec<ConflictRow>, sqlx::Error> {
    sqlx::query_as::<_, ConflictRow>(
        r#"
        SELECT tc.id, tc.conflict_type,
               tc.trip_id_1, t1.trip_code AS trip_code_1,
               tc.trip_id_2, t2.trip_code AS trip_code_2,
               tc.description, tc.severity, tc.status,
               tc.detected_at, tc.acknowledged_at, tc.resolved_at, tc.notes
        FROM   ops.trip_conflicts tc
        JOIN   ops.trips t1 ON t1.id = tc.trip_id_1
        LEFT   JOIN ops.trips t2 ON t2.id = tc.trip_id_2
        WHERE  (tc.trip_id_1 = $1 OR tc.trip_id_2 = $1)
          AND  tc.status = 'open'
        ORDER  BY tc.severity DESC, tc.detected_at DESC
        "#,
    )
    .bind(trip_id)
    .fetch_all(pool)
    .await
}

/// Load all open conflicts, optionally filtered by severity.
pub async fn fetch_all_open_conflicts(
    pool:     &PgPool,
    severity: Option<&str>,
) -> Result<Vec<ConflictRow>, sqlx::Error> {
    sqlx::query_as::<_, ConflictRow>(
        r#"
        SELECT tc.id, tc.conflict_type,
               tc.trip_id_1, t1.trip_code AS trip_code_1,
               tc.trip_id_2, t2.trip_code AS trip_code_2,
               tc.description, tc.severity, tc.status,
               tc.detected_at, tc.acknowledged_at, tc.resolved_at, tc.notes
        FROM   ops.trip_conflicts tc
        JOIN   ops.trips t1 ON t1.id = tc.trip_id_1
        LEFT   JOIN ops.trips t2 ON t2.id = tc.trip_id_2
        WHERE  tc.status = 'open'
          AND  ($1::TEXT IS NULL OR tc.severity = $1)
        ORDER  BY
               CASE tc.severity WHEN 'critical' THEN 0 ELSE 1 END ASC,
               tc.detected_at DESC
        "#,
    )
    .bind(severity)
    .fetch_all(pool)
    .await
}

// ── Private detection helpers ─────────────────────────────────────────────────

/// Interval overlap: [a_dep, a_arr) ∩ [b_dep, b_arr) ≠ ∅
///   ⟺  a_dep < b_arr  AND  a_arr > b_dep
async fn detect_driver_overlap(
    pool:      &PgPool,
    trip:      &TripCheckInput,
    driver_id: Uuid,
) -> Result<Vec<ConflictDetail>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id:                   Uuid,
        trip_code:            String,
        scheduled_departure:  DateTime<Utc>,
        scheduled_arrival:    DateTime<Utc>,
    }

    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT t.id, t.trip_code, t.scheduled_departure, t.scheduled_arrival
        FROM   ops.trips t
        WHERE  t.assigned_driver_id  = $1
          AND  t.id                 != $2
          AND  t.deleted_at          IS NULL
          AND  t.status             NOT IN ('cancelled', 'completed')
          AND  t.scheduled_departure  < $4   -- overlaps: this.dep < other.arr
          AND  t.scheduled_arrival    > $3   -- overlaps: this.arr > other.dep
        "#,
    )
    .bind(driver_id)
    .bind(trip.trip_id)
    .bind(trip.scheduled_departure)
    .bind(trip.scheduled_arrival)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let overlap_start = trip.scheduled_departure.max(row.scheduled_departure);
            let overlap_end   = trip.scheduled_arrival.min(row.scheduled_arrival);
            let overlap_mins  = (overlap_end - overlap_start).num_minutes().max(0);

            ConflictDetail {
                conflict_type:  "driver_overlap",
                trip_id_1:      trip.trip_id,
                trip_id_1_code: trip.trip_code.clone(),
                trip_id_2:      Some(row.id),
                trip_id_2_code: Some(row.trip_code.clone()),
                description: format!(
                    "Driver is double-booked: trips '{}' and '{}' overlap by {} minute(s) \
                     [{} → {}].",
                    trip.trip_code, row.trip_code, overlap_mins,
                    overlap_start.format("%H:%M"),
                    overlap_end.format("%H:%M")
                ),
                severity: "critical",
            }
        })
        .collect())
}

/// Headway check: two trips on the same route departing within the warning window.
async fn detect_route_headway(
    pool: &PgPool,
    trip: &TripCheckInput,
) -> Result<Vec<ConflictDetail>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id:                  Uuid,
        trip_code:           String,
        scheduled_departure: DateTime<Utc>,
    }

    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT t.id, t.trip_code, t.scheduled_departure
        FROM   ops.trips t
        WHERE  t.route_id           = $1
          AND  t.id                != $2
          AND  t.deleted_at         IS NULL
          AND  t.status            NOT IN ('cancelled', 'completed')
          AND  t.scheduled_departure BETWEEN
               ($3::TIMESTAMPTZ - ($4 * interval '1 second'))
           AND ($3::TIMESTAMPTZ + ($4 * interval '1 second'))
        ORDER  BY ABS(EXTRACT(EPOCH FROM (t.scheduled_departure - $3::TIMESTAMPTZ)))
        "#,
    )
    .bind(trip.route_id)
    .bind(trip.trip_id)
    .bind(trip.scheduled_departure)
    .bind(HEADWAY_WARNING_SECS)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let gap_secs = (trip.scheduled_departure - row.scheduled_departure)
                .num_seconds()
                .abs();

            let severity: &'static str = if gap_secs < HEADWAY_CRITICAL_SECS {
                "critical"
            } else if gap_secs < HEADWAY_WARNING_SECS {
                "warning"
            } else {
                return None;
            };

            Some(ConflictDetail {
                conflict_type:  "route_headway",
                trip_id_1:      trip.trip_id,
                trip_id_1_code: trip.trip_code.clone(),
                trip_id_2:      Some(row.id),
                trip_id_2_code: Some(row.trip_code.clone()),
                description: format!(
                    "Headway violation on route: trips '{}' and '{}' depart only {}s apart \
                     (minimum: {}s, recommended: {}s).",
                    trip.trip_code, row.trip_code, gap_secs,
                    HEADWAY_CRITICAL_SECS, HEADWAY_WARNING_SECS
                ),
                severity,
            })
        })
        .collect())
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(dep_offset_mins: i64, arr_offset_mins: i64) -> TripCheckInput {
        let now = Utc::now();
        TripCheckInput {
            trip_id:             Uuid::new_v4(),
            trip_code:           "T001".to_string(),
            route_id:            Uuid::new_v4(),
            assigned_driver_id:  Some(Uuid::new_v4()),
            scheduled_departure: now + chrono::Duration::minutes(dep_offset_mins),
            scheduled_arrival:   now + chrono::Duration::minutes(arr_offset_mins),
        }
    }

    #[test]
    fn overlap_formula_sanity() {
        // [10, 50] overlaps [30, 70] ⟹ dep(30) < arr(50) AND arr(70) > dep(10) ✓
        let a_dep = 10i64;
        let a_arr = 50i64;
        let b_dep = 30i64;
        let b_arr = 70i64;
        assert!(a_dep < b_arr && a_arr > b_dep);
    }

    #[test]
    fn no_overlap_formula_sanity() {
        // [10, 20] does NOT overlap [30, 40]
        let a_dep = 10i64;
        let a_arr = 20i64;
        let b_dep = 30i64;
        let b_arr = 40i64;
        assert!(!(a_dep < b_arr && a_arr > b_dep));
    }

    #[test]
    fn approaching_severity_threshold() {
        let mins_away = 8i64;
        let sev = if mins_away < APPROACHING_CRITICAL_MINS { "critical" } else { "warning" };
        assert_eq!(sev, "critical");
    }
}
