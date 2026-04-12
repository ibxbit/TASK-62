/// Handlers for Routes and Stops.
///
/// Status state machine:
///   draft ──publish──▶ active ──unpublish──▶ draft
///   draft ──schedule──▶ scheduled ──(time passes / background job)──▶ active
///   * Delete transitions any status → soft-deleted (deleted_at IS NOT NULL)
use actix_web::{web, HttpResponse};
use uuid::Uuid;

use crate::{
    auth::middleware::AuthSession,
    error::AppError,
    ops::models::{
        CreateRouteRequest, CreateStopRequest, ListQuery, ListResponse, OkResponse,
        RouteDetailResponse, RouteResponse, RouteRow, ScheduleRequest, StopResponse, StopRow,
        UpdateRouteRequest, UpdateStopRequest,
    },
    rbac::permissions::Permission,
    AppState,
};

// ============================================================
// Route handlers
// ============================================================

/// GET /ops/routes
pub async fn list_routes(
    state:   web::Data<AppState>,
    session: AuthSession,
    query:   web::Query<ListQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsRoutesRead)?;

    let limit  = query.limit();
    let offset = query.offset();

    let rows = sqlx::query_as::<_, RouteRow>(
        r#"
        SELECT id, code, name, description, status, effective_from,
               entity_version, created_at, updated_at
        FROM   ops.routes
        WHERE  deleted_at IS NULL
          AND  ($1::TEXT IS NULL OR status = $1)
          AND  ($2::TEXT IS NULL OR name ILIKE '%' || $2 || '%'
                                 OR code ILIKE '%' || $2 || '%')
        ORDER BY created_at DESC
        LIMIT $3 OFFSET $4
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
        SELECT COUNT(*) FROM ops.routes
        WHERE  deleted_at IS NULL
          AND  ($1::TEXT IS NULL OR status = $1)
          AND  ($2::TEXT IS NULL OR name ILIKE '%' || $2 || '%'
                                 OR code ILIKE '%' || $2 || '%')
        "#,
    )
    .bind(query.status.as_deref())
    .bind(query.search.as_deref())
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(ListResponse {
        data:     rows.into_iter().map(RouteResponse::from).collect(),
        total,
        page:     query.page.unwrap_or(1),
        per_page: limit,
    }))
}

/// POST /ops/routes
pub async fn create_route(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<CreateRouteRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsRoutesWrite)?;

    if body.code.trim().is_empty() || body.name.trim().is_empty() {
        return Err(AppError::BadRequest("code and name are required".to_string()));
    }

    // If effective_from is in the future, start as 'scheduled'; else 'draft'
    let status = match body.effective_from {
        Some(t) if t > chrono::Utc::now() => "scheduled",
        _ => "draft",
    };

    let row = sqlx::query_as::<_, RouteRow>(
        r#"
        INSERT INTO ops.routes (code, name, description, status, effective_from, created_by)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, code, name, description, status, effective_from,
                  entity_version, created_at, updated_at
        "#,
    )
    .bind(&body.code)
    .bind(&body.name)
    .bind(&body.description)
    .bind(status)
    .bind(body.effective_from)
    .bind(session.user_id)
    .fetch_one(&state.db)
    .await?;

    tracing::info!(route_id = %row.id, code = %row.code, user_id = %session.user_id, "Route created");

    Ok(HttpResponse::Created().json(RouteResponse::from(row)))
}

/// GET /ops/routes/{id}  — includes stops
pub async fn get_route(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsRoutesRead)?;

    let route_id = path.into_inner();

    let row = sqlx::query_as::<_, RouteRow>(
        r#"
        SELECT id, code, name, description, status, effective_from,
               entity_version, created_at, updated_at
        FROM   ops.routes
        WHERE  id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(route_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Route {} not found", route_id)))?;

    let stops = fetch_stops_for_route(&state, route_id).await?;

    Ok(HttpResponse::Ok().json(RouteDetailResponse {
        id:             row.id,
        code:           row.code,
        name:           row.name,
        description:    row.description,
        status:         row.status,
        effective_from: row.effective_from,
        version:        row.entity_version,
        stops,
        created_at:     row.created_at,
        updated_at:     row.updated_at,
    }))
}

/// PUT /ops/routes/{id}
pub async fn update_route(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<UpdateRouteRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsRoutesWrite)?;

    let route_id = path.into_inner();
    let _row = require_route_not_active_for_edit(&state, route_id).await?;

    let row = sqlx::query_as::<_, RouteRow>(
        r#"
        UPDATE ops.routes
        SET    name           = COALESCE($2, name),
               description    = COALESCE($3, description),
               effective_from = COALESCE($4, effective_from),
               entity_version = entity_version + 1,
               updated_at     = now()
        WHERE  id = $1 AND deleted_at IS NULL
        RETURNING id, code, name, description, status, effective_from,
                  entity_version, created_at, updated_at
        "#,
    )
    .bind(route_id)
    .bind(body.name.as_deref())
    .bind(body.description.as_deref())
    .bind(body.effective_from)
    .fetch_one(&state.db)
    .await?;

    tracing::info!(route_id = %row.id, version = row.entity_version, "Route updated");
    let _ = row;

    Ok(HttpResponse::Ok().json(RouteResponse::from(row)))
}

/// DELETE /ops/routes/{id}  (soft delete)
pub async fn delete_route(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsRoutesDelete)?;

    let route_id = path.into_inner();

    let affected = sqlx::query(
        "UPDATE ops.routes SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(route_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Route {} not found", route_id)));
    }

    Ok(HttpResponse::Ok().json(OkResponse { message: "Route deleted" }))
}

/// POST /ops/routes/{id}/publish
/// Transitions draft → active immediately.
pub async fn publish_route(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsRoutesWrite)?;

    let route_id = path.into_inner();

    let affected = sqlx::query(
        r#"
        UPDATE ops.routes
        SET    status      = 'active',
               updated_at  = now()
        WHERE  id          = $1
          AND  deleted_at  IS NULL
          AND  status IN ('draft', 'scheduled')
        "#,
    )
    .bind(route_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Route {} not found or not in draft/scheduled status", route_id)));
    }

    tracing::info!(route_id = %route_id, "Route published");
    Ok(HttpResponse::Ok().json(OkResponse { message: "Route published" }))
}

/// POST /ops/routes/{id}/unpublish
/// Transitions active → draft. Depots using this route continue until next sync.
pub async fn unpublish_route(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsRoutesWrite)?;

    let route_id = path.into_inner();

    let affected = sqlx::query(
        r#"
        UPDATE ops.routes
        SET    status      = 'draft',
               updated_at  = now()
        WHERE  id          = $1
          AND  deleted_at  IS NULL
          AND  status      = 'active'
        "#,
    )
    .bind(route_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Route {} not found or not currently active", route_id)));
    }

    Ok(HttpResponse::Ok().json(OkResponse { message: "Route unpublished" }))
}

/// POST /ops/routes/{id}/schedule
/// Sets status = 'scheduled' with a future effective_from timestamp.
/// A background scheduler job (outside this handler) transitions it to 'active'.
pub async fn schedule_route(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<ScheduleRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsRoutesWrite)?;

    let route_id = path.into_inner();

    if body.effective_from <= chrono::Utc::now() {
        return Err(AppError::BadRequest(
            "effective_from must be in the future".to_string(),
        ));
    }

    let affected = sqlx::query(
        r#"
        UPDATE ops.routes
        SET    status         = 'scheduled',
               effective_from = $2,
               updated_at     = now()
        WHERE  id             = $1
          AND  deleted_at     IS NULL
          AND  status         IN ('draft', 'scheduled')
        "#,
    )
    .bind(route_id)
    .bind(body.effective_from)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Route {} not found or cannot be scheduled", route_id)));
    }

    Ok(HttpResponse::Ok().json(OkResponse { message: "Route scheduled for activation" }))
}

// ============================================================
// Stop handlers (nested under /ops/routes/{route_id}/stops)
// ============================================================

/// GET /ops/routes/{route_id}/stops
pub async fn list_stops(
    state:    web::Data<AppState>,
    session:  AuthSession,
    path:     web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsStopsRead)?;

    let route_id = path.into_inner();
    let stops = fetch_stops_for_route(&state, route_id).await?;

    Ok(HttpResponse::Ok().json(stops))
}

/// POST /ops/routes/{route_id}/stops
pub async fn create_stop(
    state:    web::Data<AppState>,
    session:  AuthSession,
    path:     web::Path<Uuid>,
    body:     web::Json<CreateStopRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsStopsWrite)?;

    let route_id = path.into_inner();

    let row = sqlx::query_as::<_, StopRow>(
        r#"
        INSERT INTO ops.stops
            (route_id, code, name, sequence_order, latitude, longitude)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, route_id, code, name, sequence_order,
                  latitude, longitude, created_at, updated_at
        "#,
    )
    .bind(route_id)
    .bind(&body.code)
    .bind(&body.name)
    .bind(body.sequence_order)
    .bind(body.latitude)
    .bind(body.longitude)
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Created().json(StopResponse::from(row)))
}

/// GET /ops/routes/{route_id}/stops/{stop_id}
pub async fn get_stop(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsStopsRead)?;

    let (route_id, stop_id) = path.into_inner();

    let row = sqlx::query_as::<_, StopRow>(
        r#"
        SELECT id, route_id, code, name, sequence_order,
               latitude, longitude, created_at, updated_at
        FROM   ops.stops
        WHERE  id = $1 AND route_id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(stop_id)
    .bind(route_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Stop {} not found", stop_id)))?;

    Ok(HttpResponse::Ok().json(StopResponse::from(row)))
}

/// PUT /ops/routes/{route_id}/stops/{stop_id}
pub async fn update_stop(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<(Uuid, Uuid)>,
    body:    web::Json<UpdateStopRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsStopsWrite)?;

    let (route_id, stop_id) = path.into_inner();

    let row = sqlx::query_as::<_, StopRow>(
        r#"
        UPDATE ops.stops
        SET    name           = COALESCE($3, name),
               sequence_order = COALESCE($4, sequence_order),
               latitude       = COALESCE($5, latitude),
               longitude      = COALESCE($6, longitude),
               updated_at     = now()
        WHERE  id = $1 AND route_id = $2 AND deleted_at IS NULL
        RETURNING id, route_id, code, name, sequence_order,
                  latitude, longitude, created_at, updated_at
        "#,
    )
    .bind(stop_id)
    .bind(route_id)
    .bind(body.name.as_deref())
    .bind(body.sequence_order)
    .bind(body.latitude)
    .bind(body.longitude)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Stop {} not found", stop_id)))?;

    Ok(HttpResponse::Ok().json(StopResponse::from(row)))
}

/// DELETE /ops/routes/{route_id}/stops/{stop_id}
pub async fn delete_stop(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsStopsDelete)?;

    let (route_id, stop_id) = path.into_inner();

    let affected = sqlx::query(
        r#"
        UPDATE ops.stops
        SET    deleted_at = now()
        WHERE  id = $1 AND route_id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(stop_id)
    .bind(route_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Stop {} not found", stop_id)));
    }

    Ok(HttpResponse::Ok().json(OkResponse { message: "Stop deleted" }))
}

// ============================================================
// Private helpers
// ============================================================

async fn fetch_stops_for_route(
    state:    &AppState,
    route_id: Uuid,
) -> Result<Vec<StopResponse>, AppError> {
    let rows = sqlx::query_as::<_, StopRow>(
        r#"
        SELECT id, route_id, code, name, sequence_order,
               latitude, longitude, created_at, updated_at
        FROM   ops.stops
        WHERE  route_id = $1 AND deleted_at IS NULL
        ORDER  BY sequence_order ASC
        "#,
    )
    .bind(route_id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows.into_iter().map(StopResponse::from).collect())
}

/// Prevents editing an active route without first unpublishing it.
/// Returns the route row for subsequent use.
async fn require_route_not_active_for_edit(
    state:    &AppState,
    route_id: Uuid,
) -> Result<RouteRow, AppError> {
    let row = sqlx::query_as::<_, RouteRow>(
        r#"
        SELECT id, code, name, description, status, effective_from,
               entity_version, created_at, updated_at
        FROM   ops.routes
        WHERE  id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(route_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Route {} not found", route_id)))?;

    if row.status == "active" {
        return Err(AppError::BadRequest(
            "Cannot edit an active route. Unpublish it first.".to_string(),
        ));
    }

    Ok(row)
}
