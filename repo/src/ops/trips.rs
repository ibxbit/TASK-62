/// Handlers for Trips and Trip Calendars.
///
/// Trips follow the same draft/publish/schedule workflow as Routes.
/// Calendars are simple CRUD; they are attached to trips via `calendar_id`.
use actix_web::{web, HttpResponse};
use serde_json::json;
use uuid::Uuid;

use crate::{
    auth::middleware::AuthSession,
    error::AppError,
    ops::models::{
        CalendarResponse, CalendarRow, CreateCalendarRequest, CreateTripRequest, ListQuery,
        ListResponse, OkResponse, ScheduleRequest, TripResponse, TripRow, UpdateCalendarRequest,
        UpdateTripRequest,
    },
    rbac::permissions::Permission,
    AppState,
};

// ============================================================
// Trip handlers
// ============================================================

/// GET /ops/trips
pub async fn list_trips(
    state:   web::Data<AppState>,
    session: AuthSession,
    query:   web::Query<ListQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsRead)?;

    let limit  = query.limit();
    let offset = query.offset();

    let rows = sqlx::query_as::<_, TripRow>(
        r#"
        SELECT id, route_id, trip_code, status,
               scheduled_departure, scheduled_arrival,
               actual_departure, actual_arrival,
               assigned_driver_id, calendar_id,
               effective_from, entity_version, created_at, updated_at
        FROM   ops.trips
        WHERE  deleted_at IS NULL
          AND  ($1::TEXT IS NULL OR status = $1)
          AND  ($2::TEXT IS NULL OR trip_code ILIKE '%' || $2 || '%')
        ORDER  BY scheduled_departure DESC
        LIMIT  $3 OFFSET $4
        "#,
    )
    .bind(query.status.as_deref())
    .bind(query.search.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM ops.trips
        WHERE  deleted_at IS NULL
          AND  ($1::TEXT IS NULL OR status = $1)
          AND  ($2::TEXT IS NULL OR trip_code ILIKE '%' || $2 || '%')
        "#,
    )
    .bind(query.status.as_deref())
    .bind(query.search.as_deref())
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(ListResponse {
        data:     rows.into_iter().map(TripResponse::from).collect(),
        total,
        page:     query.page.unwrap_or(1),
        per_page: limit,
    }))
}

/// POST /ops/trips
pub async fn create_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<CreateTripRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    if body.scheduled_arrival <= body.scheduled_departure {
        return Err(AppError::BadRequest(
            "scheduled_arrival must be after scheduled_departure".to_string(),
        ));
    }

    let status = match body.effective_from {
        Some(t) if t > chrono::Utc::now() => "draft",   // pre-scheduled
        _ => "draft",
    };

    let row = sqlx::query_as::<_, TripRow>(
        r#"
        INSERT INTO ops.trips
            (route_id, trip_code, status, scheduled_departure, scheduled_arrival,
             assigned_driver_id, calendar_id, effective_from, created_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
        RETURNING id, route_id, trip_code, status,
                  scheduled_departure, scheduled_arrival,
                  actual_departure, actual_arrival,
                  assigned_driver_id, calendar_id,
                  effective_from, entity_version, created_at, updated_at
        "#,
    )
    .bind(body.route_id)
    .bind(&body.trip_code)
    .bind(status)
    .bind(body.scheduled_departure)
    .bind(body.scheduled_arrival)
    .bind(body.assigned_driver_id)
    .bind(body.calendar_id)
    .bind(body.effective_from)
    .bind(session.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Created().json(TripResponse::from(row)))
}

/// GET /ops/trips/{id}
pub async fn get_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsRead)?;

    let trip_id = path.into_inner();

    let row = sqlx::query_as::<_, TripRow>(
        r#"
        SELECT id, route_id, trip_code, status,
               scheduled_departure, scheduled_arrival,
               actual_departure, actual_arrival,
               assigned_driver_id, calendar_id,
               effective_from, entity_version, created_at, updated_at
        FROM   ops.trips
        WHERE  id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(trip_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Trip {} not found", trip_id)))?;

    Ok(HttpResponse::Ok().json(TripResponse::from(row)))
}

/// PUT /ops/trips/{id}
pub async fn update_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<UpdateTripRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let trip_id = path.into_inner();

    // Cannot edit a trip that is in_progress or completed
    let current = sqlx::query_scalar::<_, String>(
        "SELECT status FROM ops.trips WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(trip_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Trip {} not found", trip_id)))?;

    if matches!(current.as_str(), "in_progress" | "completed") {
        return Err(AppError::BadRequest(
            "Cannot edit a trip that is in_progress or completed".to_string(),
        ));
    }

    let row = sqlx::query_as::<_, TripRow>(
        r#"
        UPDATE ops.trips
        SET    scheduled_departure = COALESCE($2, scheduled_departure),
               scheduled_arrival   = COALESCE($3, scheduled_arrival),
               assigned_driver_id  = COALESCE($4, assigned_driver_id),
               calendar_id         = COALESCE($5, calendar_id),
               effective_from      = COALESCE($6, effective_from),
               entity_version      = entity_version + 1,
               updated_at          = now()
        WHERE  id = $1 AND deleted_at IS NULL
        RETURNING id, route_id, trip_code, status,
                  scheduled_departure, scheduled_arrival,
                  actual_departure, actual_arrival,
                  assigned_driver_id, calendar_id,
                  effective_from, entity_version, created_at, updated_at
        "#,
    )
    .bind(trip_id)
    .bind(body.scheduled_departure)
    .bind(body.scheduled_arrival)
    .bind(body.assigned_driver_id)
    .bind(body.calendar_id)
    .bind(body.effective_from)
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(TripResponse::from(row)))
}

/// DELETE /ops/trips/{id}
pub async fn delete_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsDelete)?;

    let trip_id = path.into_inner();

    let affected = sqlx::query(
        "UPDATE ops.trips SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(trip_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Trip {} not found", trip_id)));
    }

    Ok(HttpResponse::Ok().json(OkResponse { message: "Trip deleted" }))
}

/// POST /ops/trips/{id}/publish  — draft → published
pub async fn publish_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let trip_id = path.into_inner();

    let affected = sqlx::query(
        r#"
        UPDATE ops.trips
        SET    status = 'published', updated_at = now()
        WHERE  id = $1 AND deleted_at IS NULL AND status IN ('draft','scheduled')
        "#,
    )
    .bind(trip_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::BadRequest(
            "Trip not found or already published/active".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(OkResponse { message: "Trip published" }))
}

/// POST /ops/trips/{id}/unpublish  — published → draft
pub async fn unpublish_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let trip_id = path.into_inner();

    let affected = sqlx::query(
        r#"
        UPDATE ops.trips
        SET    status = 'draft', updated_at = now()
        WHERE  id = $1 AND deleted_at IS NULL AND status = 'published'
        "#,
    )
    .bind(trip_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::BadRequest(
            "Trip not found or is not published".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(OkResponse { message: "Trip unpublished" }))
}

/// POST /ops/trips/{id}/schedule
pub async fn schedule_trip(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<ScheduleRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsTripsWrite)?;

    let trip_id = path.into_inner();

    if body.effective_from <= chrono::Utc::now() {
        return Err(AppError::BadRequest(
            "effective_from must be in the future".to_string(),
        ));
    }

    let affected = sqlx::query(
        r#"
        UPDATE ops.trips
        SET    status         = 'scheduled',
               effective_from = $2,
               updated_at     = now()
        WHERE  id = $1 AND deleted_at IS NULL AND status = 'draft'
        "#,
    )
    .bind(trip_id)
    .bind(body.effective_from)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::BadRequest(
            "Trip not found or is not in draft status".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(json!({
        "message":        "Trip scheduled for publication",
        "effective_from": body.effective_from
    })))
}

// ============================================================
// Calendar handlers
// ============================================================

/// GET /ops/calendars
pub async fn list_calendars(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigRead)?;

    let rows = sqlx::query_as::<_, CalendarRow>(
        r#"
        SELECT id, name, description, days_of_week, valid_from, valid_to,
               exception_dates, created_at, updated_at
        FROM   ops.trip_calendars
        WHERE  deleted_at IS NULL
        ORDER  BY valid_from DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<CalendarResponse> = rows.into_iter().map(CalendarResponse::from).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// POST /ops/calendars
pub async fn create_calendar(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<CreateCalendarRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigWrite)?;

    if body.days_of_week.is_empty() {
        return Err(AppError::BadRequest("days_of_week must not be empty".to_string()));
    }

    let exceptions = match &body.exception_dates {
        Some(e) => serde_json::to_value(e).unwrap_or(json!({"included":[],"excluded":[]})),
        None    => json!({"included":[],"excluded":[]}),
    };

    let row = sqlx::query_as::<_, CalendarRow>(
        r#"
        INSERT INTO ops.trip_calendars
            (name, description, days_of_week, valid_from, valid_to,
             exception_dates, created_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7)
        RETURNING id, name, description, days_of_week, valid_from, valid_to,
                  exception_dates, created_at, updated_at
        "#,
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.days_of_week)
    .bind(body.valid_from)
    .bind(body.valid_to)
    .bind(exceptions)
    .bind(session.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Created().json(CalendarResponse::from(row)))
}

/// GET /ops/calendars/{id}
pub async fn get_calendar(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigRead)?;

    let cal_id = path.into_inner();

    let row = sqlx::query_as::<_, CalendarRow>(
        r#"
        SELECT id, name, description, days_of_week, valid_from, valid_to,
               exception_dates, created_at, updated_at
        FROM   ops.trip_calendars
        WHERE  id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(cal_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Calendar {} not found", cal_id)))?;

    Ok(HttpResponse::Ok().json(CalendarResponse::from(row)))
}

/// PUT /ops/calendars/{id}
pub async fn update_calendar(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<UpdateCalendarRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigWrite)?;

    let cal_id = path.into_inner();

    let exceptions = body
        .exception_dates
        .as_ref()
        .map(|e| serde_json::to_value(e).unwrap_or(json!({"included":[],"excluded":[]})));

    let row = sqlx::query_as::<_, CalendarRow>(
        r#"
        UPDATE ops.trip_calendars
        SET    name            = COALESCE($2, name),
               description     = COALESCE($3, description),
               days_of_week    = COALESCE($4, days_of_week),
               valid_from      = COALESCE($5, valid_from),
               valid_to        = COALESCE($6, valid_to),
               exception_dates = COALESCE($7, exception_dates),
               updated_at      = now()
        WHERE  id = $1 AND deleted_at IS NULL
        RETURNING id, name, description, days_of_week, valid_from, valid_to,
                  exception_dates, created_at, updated_at
        "#,
    )
    .bind(cal_id)
    .bind(body.name.as_deref())
    .bind(body.description.as_deref())
    .bind(body.days_of_week.as_deref())
    .bind(body.valid_from)
    .bind(body.valid_to)
    .bind(exceptions)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Calendar {} not found", cal_id)))?;

    Ok(HttpResponse::Ok().json(CalendarResponse::from(row)))
}

/// DELETE /ops/calendars/{id}
pub async fn delete_calendar(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigWrite)?;

    let cal_id = path.into_inner();

    let affected = sqlx::query(
        r#"
        UPDATE ops.trip_calendars
        SET    deleted_at = now()
        WHERE  id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(cal_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Calendar {} not found", cal_id)));
    }

    Ok(HttpResponse::Ok().json(OkResponse { message: "Calendar deleted" }))
}
