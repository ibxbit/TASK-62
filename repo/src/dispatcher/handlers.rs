use actix_web::{web, HttpResponse};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    auth::middleware::AuthSession,
    dispatcher::{
        conflicts::{
            check_and_save, detect_approaching_unassigned, detect_conflicts,
            fetch_all_open_conflicts, fetch_conflicts_for_trip, save_conflicts,
            TripCheckInput,
        },
        events,
        models::{
            ApproachingCheckResult, AssignDriverRequest, CancelTripRequest, CompleteTripRequest,
            ConflictCheckResult, ConflictResponse, DashboardResponse, PatchTripRequest,
            ResolveConflictRequest, StartTripRequest, TripSummary, TripSummaryRow, UpcomingQuery,
        },
    },
    error::AppError,
    ops::models::OkResponse,
    rbac::permissions::Permission,
    AppState,
};

// ── SQL helpers ───────────────────────────────────────────────────────────────

/// Join query used for all trip summary responses (includes route name + driver name).
const TRIP_SUMMARY_SELECT: &str = r#"
    SELECT t.id,
           t.trip_code,
           r.name              AS route_name,
           t.status,
           t.scheduled_departure,
           t.scheduled_arrival,
           t.actual_departure,
           t.actual_arrival,
           t.assigned_driver_id,
           u.username          AS driver_username
    FROM   ops.trips t
    JOIN   ops.routes r ON r.id = t.route_id
    LEFT   JOIN auth.users u ON u.id = t.assigned_driver_id
"#;

async fn to_summary(row: TripSummaryRow, pool: &sqlx::PgPool) -> TripSummary {
    let minutes_until_start = (row.scheduled_departure - Utc::now())
        .num_minutes()
        .max(0);

    let has_open_conflicts: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM ops.trip_conflicts
            WHERE (trip_id_1 = $1 OR trip_id_2 = $1) AND status = 'open'
        )
        "#,
    )
    .bind(row.id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    TripSummary {
        id:                  row.id,
        trip_code:           row.trip_code,
        route_name:          row.route_name,
        status:              row.status,
        scheduled_departure: row.scheduled_departure,
        scheduled_arrival:   row.scheduled_arrival,
        actual_departure:    row.actual_departure,
        actual_arrival:      row.actual_arrival,
        assigned_driver_id:  row.assigned_driver_id,
        driver_username:     row.driver_username,
        minutes_until_start,
        has_open_conflicts,
    }
}

async fn load_trip_check_input(
    state:   &AppState,
    trip_id: Uuid,
) -> Result<TripCheckInput, AppError> {
    #[derive(sqlx::FromRow)]
    struct TripInputRow {
        trip_code:           String,
        route_id:            Uuid,
        assigned_driver_id:  Option<Uuid>,
        scheduled_departure: chrono::DateTime<Utc>,
        scheduled_arrival:   chrono::DateTime<Utc>,
    }

    let row = sqlx::query_as::<_, TripInputRow>(
        r#"
        SELECT trip_code, route_id, assigned_driver_id,
               scheduled_departure, scheduled_arrival
        FROM   ops.trips
        WHERE  id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(trip_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Trip {} not found", trip_id)))?;

    Ok(TripCheckInput {
        trip_id,
        trip_code:           row.trip_code,
        route_id:            row.route_id,
        assigned_driver_id:  row.assigned_driver_id,
        scheduled_departure: row.scheduled_departure,
        scheduled_arrival:   row.scheduled_arrival,
    })
}

// ============================================================
// Trip management
// ============================================================

/// PATCH /dispatcher/trips/{id}
///
/// Partial update of schedule and/or driver. Automatically runs conflict
/// detection after applying changes and emits `ops.trip.modified`.
pub async fn patch_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<PatchTripRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let trip_id = path.into_inner();

    // Validate time range if both sides are provided
    if let (Some(dep), Some(arr)) = (body.scheduled_departure, body.scheduled_arrival) {
        if arr <= dep {
            return Err(AppError::BadRequest(
                "scheduled_arrival must be after scheduled_departure".to_string(),
            ));
        }
    }

    // Apply partial update
    #[derive(sqlx::FromRow)]
    struct Updated {
        trip_code:           String,
        route_id:            Uuid,
        assigned_driver_id:  Option<Uuid>,
        scheduled_departure: chrono::DateTime<Utc>,
        scheduled_arrival:   chrono::DateTime<Utc>,
    }

    let updated = sqlx::query_as::<_, Updated>(
        r#"
        UPDATE ops.trips
        SET    scheduled_departure = COALESCE($2, scheduled_departure),
               scheduled_arrival   = COALESCE($3, scheduled_arrival),
               assigned_driver_id  = COALESCE($4, assigned_driver_id),
               entity_version      = entity_version + 1,
               updated_at          = now()
        WHERE  id = $1 AND deleted_at IS NULL
          AND  status NOT IN ('completed', 'cancelled')
        RETURNING trip_code, route_id, assigned_driver_id,
                  scheduled_departure, scheduled_arrival
        "#,
    )
    .bind(trip_id)
    .bind(body.scheduled_departure)
    .bind(body.scheduled_arrival)
    .bind(body.assigned_driver_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| {
        AppError::NotFound("Trip not found or cannot be modified in its current status".to_string())
    })?;

    // Add dispatcher note if provided
    if let Some(note) = &body.note {
        insert_note(&state, trip_id, note, "general", session.user_id).await?;
    }

    // Run conflict detection on updated values
    let check = TripCheckInput {
        trip_id,
        trip_code:           updated.trip_code.clone(),
        route_id:            updated.route_id,
        assigned_driver_id:  updated.assigned_driver_id,
        scheduled_departure: updated.scheduled_departure,
        scheduled_arrival:   updated.scheduled_arrival,
    };

    let new_conflict_ids = check_and_save(&state.db, &check).await?;

    // Emit events
    events::emit(
        &state.db,
        events::TRIP_MODIFIED,
        Some(trip_id),
        Some(session.user_id),
        json!({
            "assigned_driver_id": body.assigned_driver_id,
            "scheduled_departure": body.scheduled_departure,
            "scheduled_arrival":   body.scheduled_arrival,
        }),
    )
    .await;

    for conflict_id in &new_conflict_ids {
        events::emit(
            &state.db,
            events::TRIP_CONFLICT_DETECTED,
            Some(trip_id),
            None,
            json!({ "conflict_id": conflict_id }),
        )
        .await;
    }

    tracing::info!(
        trip_id = %trip_id,
        new_conflicts = new_conflict_ids.len(),
        "Dispatcher patched trip"
    );

    Ok(HttpResponse::Ok().json(json!({
        "message":       "Trip updated",
        "new_conflicts": new_conflict_ids.len(),
        "conflict_ids":  new_conflict_ids,
    })))
}

/// POST /dispatcher/trips/{id}/assign
///
/// Assign or reassign a driver. Runs driver-overlap conflict check before
/// committing. Emits `ops.trip.driver_assigned`.
pub async fn assign_driver(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<AssignDriverRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let trip_id   = path.into_inner();
    let driver_id = body.driver_id;

    // Verify driver exists and is active
    let driver_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM auth.users WHERE id = $1 AND is_active = TRUE AND deleted_at IS NULL)",
    )
    .bind(driver_id)
    .fetch_one(&state.db)
    .await?;

    if !driver_exists {
        return Err(AppError::BadRequest(
            "Driver not found or is inactive".to_string(),
        ));
    }

    // Load current trip data for conflict check
    let mut check = load_trip_check_input(&state, trip_id).await?;
    check.assigned_driver_id = Some(driver_id); // use the new driver for the check

    // Detect driver overlap BEFORE applying the assignment
    let detected = detect_conflicts(&state.db, &check).await?;
    let driver_conflicts: Vec<_> = detected
        .iter()
        .filter(|c| c.conflict_type == "driver_overlap")
        .collect();

    if !driver_conflicts.is_empty() {
        // Persist the conflicts but still allow the assignment (warn, don't block)
        let new_ids = save_conflicts(&state.db, &detected).await?;
        for id in &new_ids {
            events::emit(
                &state.db,
                events::TRIP_CONFLICT_DETECTED,
                Some(trip_id),
                Some(session.user_id),
                json!({ "conflict_id": id, "type": "driver_overlap" }),
            )
            .await;
        }

        tracing::warn!(
            trip_id = %trip_id,
            driver_id = %driver_id,
            conflicts = new_ids.len(),
            "Driver assigned with overlap conflicts"
        );
    }

    // Apply assignment
    sqlx::query(
        r#"
        UPDATE ops.trips
        SET    assigned_driver_id = $2,
               entity_version     = entity_version + 1,
               updated_at         = now()
        WHERE  id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(trip_id)
    .bind(driver_id)
    .execute(&state.db)
    .await?;

    if let Some(note) = &body.note {
        insert_note(&state, trip_id, note, "general", session.user_id).await?;
    }

    events::emit(
        &state.db,
        events::TRIP_DRIVER_ASSIGNED,
        Some(trip_id),
        Some(session.user_id),
        json!({ "driver_id": driver_id }),
    )
    .await;

    Ok(HttpResponse::Ok().json(json!({
        "message":       "Driver assigned",
        "driver_id":     driver_id,
        "warning_conflicts": driver_conflicts.len(),
    })))
}

/// POST /dispatcher/trips/{id}/start
///
/// Transitions trip to `in_progress`. Records actual departure.
/// Emits `ops.trip.started`.
pub async fn start_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<StartTripRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let trip_id   = path.into_inner();
    let actual_dep = body.actual_departure.unwrap_or_else(Utc::now);

    let affected = sqlx::query(
        r#"
        UPDATE ops.trips
        SET    status           = 'in_progress',
               actual_departure = $2,
               entity_version   = entity_version + 1,
               updated_at       = now()
        WHERE  id = $1
          AND  deleted_at IS NULL
          AND  status IN ('published', 'draft', 'scheduled')
        "#,
    )
    .bind(trip_id)
    .bind(actual_dep)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::BadRequest(
            "Trip not found or cannot be started in its current status".to_string(),
        ));
    }

    if let Some(note) = &body.note {
        insert_note(&state, trip_id, note, "general", session.user_id).await?;
    }

    events::emit(
        &state.db,
        events::TRIP_STARTED,
        Some(trip_id),
        Some(session.user_id),
        json!({ "actual_departure": actual_dep }),
    )
    .await;

    tracing::info!(trip_id = %trip_id, "Trip started");

    Ok(HttpResponse::Ok().json(json!({
        "message":          "Trip started",
        "actual_departure": actual_dep,
    })))
}

/// POST /dispatcher/trips/{id}/complete
///
/// Transitions trip to `completed`. Records actual arrival.
/// Auto-resolves any open `unassigned_approaching` conflicts for this trip.
/// Emits `ops.trip.completed`.
pub async fn complete_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<CompleteTripRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let trip_id    = path.into_inner();
    let actual_arr = body.actual_arrival.unwrap_or_else(Utc::now);

    let affected = sqlx::query(
        r#"
        UPDATE ops.trips
        SET    status         = 'completed',
               actual_arrival = $2,
               entity_version = entity_version + 1,
               updated_at     = now()
        WHERE  id = $1 AND deleted_at IS NULL AND status = 'in_progress'
        "#,
    )
    .bind(trip_id)
    .bind(actual_arr)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::BadRequest(
            "Trip not found or is not currently in_progress".to_string(),
        ));
    }

    // Auto-resolve all open conflicts for this trip (trip is done, conflicts are moot)
    sqlx::query(
        r#"
        UPDATE ops.trip_conflicts
        SET    status      = 'resolved',
               resolved_at = now(),
               resolved_by = $2,
               notes       = COALESCE(notes, '') || ' [auto-resolved: trip completed]',
               updated_at  = now()
        WHERE  (trip_id_1 = $1 OR trip_id_2 = $1) AND status = 'open'
        "#,
    )
    .bind(trip_id)
    .bind(session.user_id)
    .execute(&state.db)
    .await?;

    if let Some(note) = &body.note {
        insert_note(&state, trip_id, note, "general", session.user_id).await?;
    }

    events::emit(
        &state.db,
        events::TRIP_COMPLETED,
        Some(trip_id),
        Some(session.user_id),
        json!({ "actual_arrival": actual_arr }),
    )
    .await;

    Ok(HttpResponse::Ok().json(json!({
        "message":        "Trip completed",
        "actual_arrival": actual_arr,
    })))
}

/// POST /dispatcher/trips/{id}/cancel
///
/// Cancels a trip. Auto-resolves open conflicts. Emits `ops.trip.cancelled`.
pub async fn cancel_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<CancelTripRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let trip_id = path.into_inner();

    let affected = sqlx::query(
        r#"
        UPDATE ops.trips
        SET    status         = 'cancelled',
               entity_version = entity_version + 1,
               updated_at     = now()
        WHERE  id = $1 AND deleted_at IS NULL
          AND  status NOT IN ('completed', 'cancelled')
        "#,
    )
    .bind(trip_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::BadRequest(
            "Trip not found or is already completed/cancelled".to_string(),
        ));
    }

    // Auto-resolve all open conflicts
    sqlx::query(
        r#"
        UPDATE ops.trip_conflicts
        SET    status      = 'resolved',
               resolved_at = now(),
               resolved_by = $2,
               notes       = COALESCE(notes, '') || ' [auto-resolved: trip cancelled]',
               updated_at  = now()
        WHERE  (trip_id_1 = $1 OR trip_id_2 = $1) AND status = 'open'
        "#,
    )
    .bind(trip_id)
    .bind(session.user_id)
    .execute(&state.db)
    .await?;

    let reason = body.reason.as_deref().unwrap_or("No reason provided");
    insert_note(&state, trip_id, reason, "override", session.user_id).await?;

    events::emit(
        &state.db,
        events::TRIP_CANCELLED,
        Some(trip_id),
        Some(session.user_id),
        json!({ "reason": reason }),
    )
    .await;

    tracing::info!(trip_id = %trip_id, reason, "Trip cancelled");

    Ok(HttpResponse::Ok().json(OkResponse { message: "Trip cancelled" }))
}

// ============================================================
// Conflict management
// ============================================================

/// GET /dispatcher/trips/{id}/conflicts
pub async fn get_trip_conflicts(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsRead)?;

    let trip_id = path.into_inner();
    let rows    = fetch_conflicts_for_trip(&state.db, trip_id).await?;
    let resp: Vec<ConflictResponse> = rows.into_iter().map(Into::into).collect();

    Ok(HttpResponse::Ok().json(resp))
}

/// POST /dispatcher/trips/{id}/check
///
/// Runs conflict detection on-demand for one trip and persists any new conflicts.
pub async fn check_trip_conflicts(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsRead)?;

    let trip_id = path.into_inner();
    let check   = load_trip_check_input(&state, trip_id).await?;
    let new_ids = check_and_save(&state.db, &check).await?;

    for id in &new_ids {
        events::emit(
            &state.db,
            events::TRIP_CONFLICT_DETECTED,
            Some(trip_id),
            Some(session.user_id),
            json!({ "conflict_id": id }),
        )
        .await;
    }

    let conflicts = fetch_conflicts_for_trip(&state.db, trip_id).await?;
    let resp: Vec<ConflictResponse> = conflicts.into_iter().map(Into::into).collect();

    Ok(HttpResponse::Ok().json(ConflictCheckResult {
        trip_id,
        new_conflicts: new_ids.len(),
        conflicts:     resp,
    }))
}

/// GET /dispatcher/conflicts?severity=critical
pub async fn list_conflicts(
    state:   web::Data<AppState>,
    session: AuthSession,
    query:   web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsRead)?;

    let severity = query.get("severity").map(|s| s.as_str());
    let rows     = fetch_all_open_conflicts(&state.db, severity).await?;
    let resp: Vec<ConflictResponse> = rows.into_iter().map(Into::into).collect();

    Ok(HttpResponse::Ok().json(resp))
}

/// POST /dispatcher/conflicts/{id}/acknowledge
pub async fn acknowledge_conflict(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let conflict_id = path.into_inner();

    let affected = sqlx::query(
        r#"
        UPDATE ops.trip_conflicts
        SET    status           = 'acknowledged',
               acknowledged_at  = now(),
               acknowledged_by  = $2,
               updated_at       = now()
        WHERE  id = $1 AND status = 'open'
        "#,
    )
    .bind(conflict_id)
    .bind(session.user_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::BadRequest(
            "Conflict not found or is not open".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(OkResponse { message: "Conflict acknowledged" }))
}

/// POST /dispatcher/conflicts/{id}/resolve
pub async fn resolve_conflict(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<ResolveConflictRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let conflict_id = path.into_inner();

    let affected = sqlx::query(
        r#"
        UPDATE ops.trip_conflicts
        SET    status      = 'resolved',
               resolved_at = now(),
               resolved_by = $2,
               notes       = COALESCE($3, notes),
               updated_at  = now()
        WHERE  id = $1 AND status IN ('open', 'acknowledged')
        "#,
    )
    .bind(conflict_id)
    .bind(session.user_id)
    .bind(body.notes.as_deref())
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::BadRequest(
            "Conflict not found or already resolved".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(OkResponse { message: "Conflict resolved" }))
}

// ============================================================
// Monitoring
// ============================================================

/// GET /dispatcher/monitor/dashboard
pub async fn dashboard(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsRead)?;

    // Parallel counts
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ops.trips WHERE status = 'in_progress' AND deleted_at IS NULL",
    )
    .fetch_one(&state.db)
    .await?;

    let upcoming_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM ops.trips
        WHERE  status IN ('published', 'draft', 'scheduled')
          AND  deleted_at IS NULL
          AND  scheduled_departure BETWEEN now() AND now() + interval '2 hours'
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    let conflicts_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ops.trip_conflicts WHERE status = 'open'",
    )
    .fetch_one(&state.db)
    .await?;

    let unassigned_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM ops.trips
        WHERE  assigned_driver_id IS NULL
          AND  status IN ('published', 'draft')
          AND  deleted_at IS NULL
          AND  scheduled_departure BETWEEN now() AND now() + interval '30 minutes'
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    // Active trip list
    let active_rows = sqlx::query_as::<_, TripSummaryRow>(
        &format!(
            "{} WHERE t.status = 'in_progress' AND t.deleted_at IS NULL \
             ORDER BY t.scheduled_departure ASC LIMIT 20",
            TRIP_SUMMARY_SELECT
        ),
    )
    .fetch_all(&state.db)
    .await?;

    let mut active_trips = Vec::new();
    for row in active_rows {
        active_trips.push(to_summary(row, &state.db).await);
    }

    // Upcoming trips (next 1 hour)
    let upcoming_rows = sqlx::query_as::<_, TripSummaryRow>(
        &format!(
            "{} WHERE t.status IN ('published','draft','scheduled') \
             AND t.deleted_at IS NULL \
             AND t.scheduled_departure BETWEEN now() AND now() + interval '1 hour' \
             ORDER BY t.scheduled_departure ASC LIMIT 20",
            TRIP_SUMMARY_SELECT
        ),
    )
    .fetch_all(&state.db)
    .await?;

    let mut upcoming_trips = Vec::new();
    for row in upcoming_rows {
        upcoming_trips.push(to_summary(row, &state.db).await);
    }

    // Recent critical/open conflicts
    let conflict_rows = fetch_all_open_conflicts(&state.db, Some("critical")).await?;
    let recent_conflicts: Vec<ConflictResponse> =
        conflict_rows.into_iter().take(10).map(Into::into).collect();

    Ok(HttpResponse::Ok().json(DashboardResponse {
        active_trips_count:       active_count,
        upcoming_2h_count:        upcoming_count,
        open_conflicts_count:     conflicts_count,
        unassigned_within_30min:  unassigned_count,
        active_trips,
        upcoming_trips,
        recent_conflicts,
    }))
}

/// GET /dispatcher/monitor/upcoming?window_minutes=120
pub async fn upcoming_trips(
    state:   web::Data<AppState>,
    session: AuthSession,
    query:   web::Query<UpcomingQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsRead)?;

    let window = query.window_minutes.unwrap_or(120).clamp(10, 480);

    let rows = sqlx::query_as::<_, TripSummaryRow>(
        &format!(
            "{} WHERE t.status IN ('published','draft','scheduled') \
             AND t.deleted_at IS NULL \
             AND t.scheduled_departure BETWEEN now() AND now() + ($1 * interval '1 minute') \
             ORDER BY t.scheduled_departure ASC",
            TRIP_SUMMARY_SELECT
        ),
    )
    .bind(window)
    .fetch_all(&state.db)
    .await?;

    let mut trips = Vec::new();
    for row in rows {
        trips.push(to_summary(row, &state.db).await);
    }

    Ok(HttpResponse::Ok().json(json!({ "window_minutes": window, "trips": trips })))
}

/// GET /dispatcher/monitor/active
pub async fn active_trips(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsRead)?;

    let rows = sqlx::query_as::<_, TripSummaryRow>(
        &format!(
            "{} WHERE t.status = 'in_progress' AND t.deleted_at IS NULL \
             ORDER BY t.actual_departure ASC NULLS LAST",
            TRIP_SUMMARY_SELECT
        ),
    )
    .fetch_all(&state.db)
    .await?;

    let mut trips = Vec::new();
    for row in rows {
        trips.push(to_summary(row, &state.db).await);
    }

    Ok(HttpResponse::Ok().json(trips))
}

/// GET /dispatcher/monitor/unassigned
///
/// Trips without an assigned driver that start within the next 30 minutes.
pub async fn unassigned_trips(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsRead)?;

    let rows = sqlx::query_as::<_, TripSummaryRow>(
        &format!(
            "{} WHERE t.assigned_driver_id IS NULL \
             AND t.status IN ('published','draft') \
             AND t.deleted_at IS NULL \
             AND t.scheduled_departure BETWEEN now() AND now() + interval '30 minutes' \
             ORDER BY t.scheduled_departure ASC",
            TRIP_SUMMARY_SELECT
        ),
    )
    .fetch_all(&state.db)
    .await?;

    let mut trips = Vec::new();
    for row in rows {
        trips.push(to_summary(row, &state.db).await);
    }

    Ok(HttpResponse::Ok().json(json!({
        "unassigned_count": trips.len(),
        "trips":            trips,
    })))
}

/// POST /dispatcher/monitor/check-approaching
///
/// Designed to be called by a scheduler (e.g. every 5 minutes).
/// Finds trips approaching their start time with no driver, creates
/// `unassigned_approaching` conflicts if not already present, and
/// emits `ops.trip.start_approaching` for ALL approaching trips
/// (assigned or not) that haven't had an event emitted in the last hour.
pub async fn check_approaching(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsRead)?;

    // 1. Detect and persist unassigned-approaching conflicts
    let approaching_conflicts = detect_approaching_unassigned(&state.db).await?;
    let conflicts_created = approaching_conflicts.len();

    for (trip_id, severity) in &approaching_conflicts {
        events::emit(
            &state.db,
            events::TRIP_CONFLICT_DETECTED,
            Some(*trip_id),
            None,
            json!({ "type": "unassigned_approaching", "severity": severity }),
        )
        .await;
    }

    // 2. Emit start_approaching events for ALL trips starting within 30 min
    //    that haven't had this event in the last hour (deduplication).
    #[derive(sqlx::FromRow)]
    struct ApproachingRow {
        id:                  Uuid,
        trip_code:           String,
        scheduled_departure: chrono::DateTime<Utc>,
    }

    let approaching = sqlx::query_as::<_, ApproachingRow>(
        r#"
        SELECT t.id, t.trip_code, t.scheduled_departure
        FROM   ops.trips t
        WHERE  t.deleted_at IS NULL
          AND  t.status IN ('published', 'draft', 'scheduled', 'in_progress')
          AND  t.scheduled_departure BETWEEN now() AND now() + interval '30 minutes'
          AND  t.id NOT IN (
              SELECT source_entity_id
              FROM   notifications.events
              WHERE  event_type    = $1
                AND  created_at   > now() - interval '1 hour'
                AND  source_entity_id IS NOT NULL
          )
        ORDER  BY t.scheduled_departure ASC
        "#,
    )
    .bind(events::TRIP_START_APPROACHING)
    .fetch_all(&state.db)
    .await?;

    let mut emitted_ids = Vec::new();

    for row in &approaching {
        let mins_away = (row.scheduled_departure - Utc::now())
            .num_minutes()
            .max(0);

        events::emit(
            &state.db,
            events::TRIP_START_APPROACHING,
            Some(row.id),
            None,
            json!({
                "trip_code":       row.trip_code,
                "minutes_away":    mins_away,
                "scheduled_departure": row.scheduled_departure,
            }),
        )
        .await;

        emitted_ids.push(row.id);
    }

    tracing::info!(
        events_emitted   = emitted_ids.len(),
        conflicts_created,
        "Approaching check completed"
    );

    Ok(HttpResponse::Ok().json(ApproachingCheckResult {
        events_emitted:       emitted_ids.len(),
        conflicts_created,
        approaching_trip_ids: emitted_ids,
    }))
}

// ── Private helpers ───────────────────────────────────────────────────────────

async fn insert_note(
    state:      &AppState,
    trip_id:    Uuid,
    note:       &str,
    note_type:  &str,
    created_by: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO ops.dispatcher_notes (trip_id, note, note_type, created_by)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(trip_id)
    .bind(note)
    .bind(note_type)
    .bind(created_by)
    .execute(&state.db)
    .await?;
    Ok(())
}
