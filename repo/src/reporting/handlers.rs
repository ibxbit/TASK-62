use actix_web::{web, HttpResponse};
use chrono::Utc;
use uuid::Uuid;

use crate::auth::middleware::{AuthSession, ReauthGuard};
use crate::error::AppError;
use crate::rbac::permissions::Permission;
use crate::AppState;
use super::export;
use super::metrics::compute_metric;
use super::models::{
    ComputeRequest, CreateMetricRequest, CreateScheduledReportRequest,
    ExportQuery, MetricDefinitionResponse, MetricQuery, ReportRunResponse,
    ScheduledReportResponse, UpdateMetricRequest, UpdateScheduledReportRequest,
};

// ============================================================
// Metric definition CRUD
// ============================================================

/// GET /reporting/metrics
pub async fn list_metrics(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::ReportingRead)?;

    let rows = sqlx::query_as!(
        super::models::MetricDefinitionRow,
        r#"
        SELECT id, metric_key, display_name, description,
               formula_type, dimension_keys, config,
               is_builtin, is_active, created_at, updated_at
        FROM reporting.metric_definitions
        WHERE is_active = TRUE
        ORDER BY display_name
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<MetricDefinitionResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// POST /reporting/metrics
/// Requires re-authentication within the last 10 minutes.
pub async fn create_metric(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    body:    web::Json<CreateMetricRequest>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::ReportingMetricsManage)?;

    let valid_types = ["on_time_departure_rate", "refund_rate",
                       "reconciliation_mismatch_count", "custom_sql"];
    if !valid_types.contains(&body.formula_type.as_str()) {
        return Err(AppError::BadRequest(
            format!("formula_type must be one of: {}", valid_types.join(", ")),
        ));
    }

    let dim_keys = body.dimension_keys.clone().unwrap_or_default();
    let config   = body.config.clone().unwrap_or(serde_json::Value::Object(Default::default()));

    let row = sqlx::query_as!(
        super::models::MetricDefinitionRow,
        r#"
        INSERT INTO reporting.metric_definitions
            (metric_key, display_name, description, formula_type, dimension_keys, config, is_builtin)
        VALUES ($1, $2, $3, $4, $5, $6, FALSE)
        RETURNING id, metric_key, display_name, description,
                  formula_type, dimension_keys, config,
                  is_builtin, is_active, created_at, updated_at
        "#,
        body.metric_key,
        body.display_name,
        body.description,
        body.formula_type,
        &dim_keys,
        config,
    )
    .fetch_one(&state.db)
    .await?;

    crate::audit::writer::log(
        &state.db,
        &session,
        crate::audit::writer::AuditEntry {
            action:       crate::audit::writer::ACTION_METRIC_CREATE,
            domain:       "reporting",
            entity_type:  "metric_definition",
            entity_id:    row.id.to_string(),
            before_state: None,
            after_state:  Some(serde_json::json!({
                "metric_key":   &row.metric_key,
                "formula_type": &row.formula_type,
            })),
            metadata:     serde_json::Value::Object(Default::default()),
        },
    )
    .await
    .unwrap_or_else(|e| tracing::warn!(error = %e, "audit log failed"));

    Ok(HttpResponse::Created().json(MetricDefinitionResponse::from(row)))
}

/// GET /reporting/metrics/{id}
pub async fn get_metric(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::ReportingRead)?;
    let id = *path;

    let row = sqlx::query_as!(
        super::models::MetricDefinitionRow,
        r#"
        SELECT id, metric_key, display_name, description,
               formula_type, dimension_keys, config,
               is_builtin, is_active, created_at, updated_at
        FROM reporting.metric_definitions
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Metric not found".to_string()))?;

    Ok(HttpResponse::Ok().json(MetricDefinitionResponse::from(row)))
}

/// PUT /reporting/metrics/{id}
/// Requires re-authentication within the last 10 minutes.
pub async fn update_metric(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<Uuid>,
    body:    web::Json<UpdateMetricRequest>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::ReportingMetricsManage)?;
    let id = *path;

    let existing = sqlx::query_as!(
        super::models::MetricDefinitionRow,
        r#"
        SELECT id, metric_key, display_name, description,
               formula_type, dimension_keys, config,
               is_builtin, is_active, created_at, updated_at
        FROM reporting.metric_definitions WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Metric not found".to_string()))?;

    if existing.is_builtin && (body.dimension_keys.is_some() || body.config.is_some()) {
        return Err(AppError::BadRequest(
            "built-in metric dimension_keys and config are immutable".to_string(),
        ));
    }

    // Capture before-state prior to partial moves of existing fields.
    let audit_before = serde_json::json!({
        "display_name": existing.display_name.clone(),
        "is_active":    existing.is_active,
    });

    let display_name   = body.display_name.clone().unwrap_or(existing.display_name);
    let description    = body.description.clone().or(existing.description);
    let dimension_keys = body.dimension_keys.clone().unwrap_or(existing.dimension_keys);
    let config         = body.config.clone().unwrap_or(existing.config);
    let is_active      = body.is_active.unwrap_or(existing.is_active);

    let row = sqlx::query_as!(
        super::models::MetricDefinitionRow,
        r#"
        UPDATE reporting.metric_definitions
        SET display_name   = $2,
            description    = $3,
            dimension_keys = $4,
            config         = $5,
            is_active      = $6,
            updated_at     = now()
        WHERE id = $1
        RETURNING id, metric_key, display_name, description,
                  formula_type, dimension_keys, config,
                  is_builtin, is_active, created_at, updated_at
        "#,
        id,
        display_name,
        description,
        &dimension_keys,
        config,
        is_active,
    )
    .fetch_one(&state.db)
    .await?;

    crate::audit::writer::log(
        &state.db,
        &session,
        crate::audit::writer::AuditEntry {
            action:       crate::audit::writer::ACTION_METRIC_UPDATE,
            domain:       "reporting",
            entity_type:  "metric_definition",
            entity_id:    id.to_string(),
            before_state: Some(audit_before),
            after_state:  Some(serde_json::json!({
                "display_name": &row.display_name,
                "is_active":    row.is_active,
            })),
            metadata:     serde_json::Value::Object(Default::default()),
        },
    )
    .await
    .unwrap_or_else(|e| tracing::warn!(error = %e, "audit log failed"));

    Ok(HttpResponse::Ok().json(MetricDefinitionResponse::from(row)))
}

/// DELETE /reporting/metrics/{id}
/// Requires re-authentication within the last 10 minutes.
pub async fn delete_metric(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::ReportingMetricsManage)?;
    let id = *path;

    let existing = sqlx::query!(
        "SELECT is_builtin FROM reporting.metric_definitions WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Metric not found".to_string()))?;

    if existing.is_builtin {
        sqlx::query!(
            "UPDATE reporting.metric_definitions SET is_active = FALSE, updated_at = now() WHERE id = $1",
            id
        )
        .execute(&state.db)
        .await?;
    } else {
        sqlx::query!("DELETE FROM reporting.metric_definitions WHERE id = $1", id)
            .execute(&state.db)
            .await?;
    }

    crate::audit::writer::log(
        &state.db,
        &session,
        crate::audit::writer::AuditEntry {
            action:       crate::audit::writer::ACTION_METRIC_DELETE,
            domain:       "reporting",
            entity_type:  "metric_definition",
            entity_id:    id.to_string(),
            before_state: Some(serde_json::json!({ "is_builtin": existing.is_builtin })),
            after_state:  None,
            metadata:     serde_json::Value::Object(Default::default()),
        },
    )
    .await
    .unwrap_or_else(|e| tracing::warn!(error = %e, "audit log failed"));

    Ok(HttpResponse::NoContent().finish())
}

// ============================================================
// Metric compute
// ============================================================

/// POST /reporting/metrics/compute
pub async fn compute_metrics(
    state:   web::Data<AppState>,
    session: AuthSession,
    body:    web::Json<ComputeRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::ReportingRead)?;

    if body.metric_ids.is_empty() {
        return Err(AppError::BadRequest("metric_ids must not be empty".to_string()));
    }
    if body.date_from >= body.date_to {
        return Err(AppError::BadRequest("date_from must be before date_to".to_string()));
    }

    let granularity = body.granularity.clone().unwrap_or_else(|| "day".to_string());
    let valid_gran  = ["hour", "day", "week", "month"];
    if !valid_gran.contains(&granularity.as_str()) {
        return Err(AppError::BadRequest(
            "granularity must be one of: hour, day, week, month".to_string(),
        ));
    }

    let mut results = Vec::new();
    for &metric_id in &body.metric_ids {
        let def = sqlx::query_as!(
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
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Metric {} not found", metric_id)))?;

        let query = MetricQuery {
            metric_id,
            date_from:   body.date_from,
            date_to:     body.date_to,
            granularity: granularity.clone(),
            route_id:    body.route_id,
            depot_id:    body.depot_id,
        };

        let result = compute_metric(&state.db, &def, &query).await?;
        results.push(result);
    }

    Ok(HttpResponse::Ok().json(results))
}

// ============================================================
// Scheduled reports CRUD
// ============================================================

/// GET /reporting/schedules
pub async fn list_schedules(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::ReportingRead)?;

    let rows = sqlx::query_as!(
        super::models::ScheduledReportRow,
        r#"
        SELECT id, name, metric_ids, schedule,
               route_id, depot_id, date_range_days,
               granularity, output_format,
               recipient_user_ids, is_active,
               next_run_at, last_run_at, created_by, created_at, updated_at
        FROM reporting.scheduled_reports
        ORDER BY name
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<ScheduledReportResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// POST /reporting/schedules
/// Requires re-authentication within the last 10 minutes.
pub async fn create_schedule(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    body:    web::Json<CreateScheduledReportRequest>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::ReportingMetricsManage)?;

    let valid_schedules = ["daily", "weekly", "monthly"];
    if !valid_schedules.contains(&body.schedule.as_str()) {
        return Err(AppError::BadRequest(
            "schedule must be one of: daily, weekly, monthly".to_string(),
        ));
    }

    let date_range_days    = body.date_range_days.unwrap_or(30);
    let granularity        = body.granularity.clone().unwrap_or_else(|| "day".to_string());
    let output_format      = body.output_format.clone().unwrap_or_else(|| "csv".to_string());
    let recipient_user_ids = body.recipient_user_ids.clone().unwrap_or_default();

    let row = sqlx::query_as!(
        super::models::ScheduledReportRow,
        r#"
        INSERT INTO reporting.scheduled_reports
            (name, metric_ids, schedule, route_id, depot_id,
             date_range_days, granularity, output_format,
             recipient_user_ids, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING id, name, metric_ids, schedule,
                  route_id, depot_id, date_range_days,
                  granularity, output_format,
                  recipient_user_ids, is_active,
                  next_run_at, last_run_at, created_by, created_at, updated_at
        "#,
        body.name,
        &body.metric_ids,
        body.schedule,
        body.route_id,
        body.depot_id,
        date_range_days,
        granularity,
        output_format,
        &recipient_user_ids,
        session.user_id,
    )
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Created().json(ScheduledReportResponse::from(row)))
}

/// GET /reporting/schedules/{id}
pub async fn get_schedule(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::ReportingRead)?;
    let id = *path;

    let row = sqlx::query_as!(
        super::models::ScheduledReportRow,
        r#"
        SELECT id, name, metric_ids, schedule,
               route_id, depot_id, date_range_days,
               granularity, output_format,
               recipient_user_ids, is_active,
               next_run_at, last_run_at, created_by, created_at, updated_at
        FROM reporting.scheduled_reports WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Schedule not found".to_string()))?;

    Ok(HttpResponse::Ok().json(ScheduledReportResponse::from(row)))
}

/// PUT /reporting/schedules/{id}
/// Requires re-authentication within the last 10 minutes.
pub async fn update_schedule(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<Uuid>,
    body:    web::Json<UpdateScheduledReportRequest>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::ReportingMetricsManage)?;
    let id = *path;

    let existing = sqlx::query_as!(
        super::models::ScheduledReportRow,
        r#"
        SELECT id, name, metric_ids, schedule,
               route_id, depot_id, date_range_days,
               granularity, output_format,
               recipient_user_ids, is_active,
               next_run_at, last_run_at, created_by, created_at, updated_at
        FROM reporting.scheduled_reports WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Schedule not found".to_string()))?;

    let name            = body.name.clone().unwrap_or(existing.name);
    let metric_ids      = body.metric_ids.clone().unwrap_or(existing.metric_ids);
    let schedule        = body.schedule.clone().unwrap_or(existing.schedule);
    let route_id        = body.route_id.or(existing.route_id);
    let depot_id        = body.depot_id.or(existing.depot_id);
    let date_range_days = body.date_range_days.unwrap_or(existing.date_range_days);
    let granularity     = body.granularity.clone().unwrap_or(existing.granularity);
    let output_format   = body.output_format.clone().unwrap_or(existing.output_format);
    let recipient_ids   = body.recipient_user_ids.clone().unwrap_or(existing.recipient_user_ids);
    let is_active       = body.is_active.unwrap_or(existing.is_active);

    let row = sqlx::query_as!(
        super::models::ScheduledReportRow,
        r#"
        UPDATE reporting.scheduled_reports
        SET name               = $2,
            metric_ids         = $3,
            schedule           = $4,
            route_id           = $5,
            depot_id           = $6,
            date_range_days    = $7,
            granularity        = $8,
            output_format      = $9,
            recipient_user_ids = $10,
            is_active          = $11,
            updated_at         = now()
        WHERE id = $1
        RETURNING id, name, metric_ids, schedule,
                  route_id, depot_id, date_range_days,
                  granularity, output_format,
                  recipient_user_ids, is_active,
                  next_run_at, last_run_at, created_by, created_at, updated_at
        "#,
        id,
        name,
        &metric_ids,
        schedule,
        route_id,
        depot_id,
        date_range_days,
        granularity,
        output_format,
        &recipient_ids,
        is_active,
    )
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Ok().json(ScheduledReportResponse::from(row)))
}

/// DELETE /reporting/schedules/{id}
/// Requires re-authentication within the last 10 minutes.
pub async fn delete_schedule(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::ReportingMetricsManage)?;
    let id = *path;

    let result = sqlx::query!(
        "DELETE FROM reporting.scheduled_reports WHERE id = $1",
        id
    )
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Schedule not found".to_string()));
    }
    Ok(HttpResponse::NoContent().finish())
}

// ============================================================
// Report runs
// ============================================================

/// POST /reporting/schedules/{id}/trigger
/// Requires re-authentication within the last 10 minutes.
pub async fn trigger_run(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::ReportingMetricsManage)?;
    let scheduled_id = *path;

    let report = sqlx::query_as!(
        super::models::ScheduledReportRow,
        r#"
        SELECT id, name, metric_ids, schedule,
               route_id, depot_id, date_range_days,
               granularity, output_format,
               recipient_user_ids, is_active,
               next_run_at, last_run_at, created_by, created_at, updated_at
        FROM reporting.scheduled_reports WHERE id = $1
        "#,
        scheduled_id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Schedule not found".to_string()))?;

    let run_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO reporting.report_runs
            (scheduled_id, trigger_user_id, metric_ids, route_id, depot_id,
             date_from, date_to, granularity, output_format, status, started_at)
        VALUES ($1, $2, $3, $4, $5,
                now() - ($6::int * INTERVAL '1 day'), now(),
                $7, $8, 'running', now())
        RETURNING id
        "#,
        report.id,
        session.user_id,
        &report.metric_ids,
        report.route_id,
        report.depot_id,
        report.date_range_days,
        report.granularity,
        report.output_format,
    )
    .fetch_one(&state.db)
    .await?;

    let pool   = state.db.clone();
    let report = report.clone();
    let uid    = session.user_id;
    tokio::spawn(async move {
        super::scheduler::run_triggered_run(&pool, &report, run_id, uid).await;
    });

    Ok(HttpResponse::Accepted().json(serde_json::json!({ "run_id": run_id })))
}

/// GET /reporting/runs
pub async fn list_runs(
    state:   web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::ReportingRead)?;

    let rows = sqlx::query_as!(
        super::models::ReportRunRow,
        r#"
        SELECT id, scheduled_id, trigger_user_id, metric_ids,
               route_id, depot_id, date_from, date_to,
               granularity, output_format, status,
               result_data, error_message, started_at, completed_at, created_at
        FROM reporting.report_runs
        ORDER BY created_at DESC
        LIMIT 50
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let resp: Vec<ReportRunResponse> = rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(resp))
}

/// GET /reporting/runs/{id}
pub async fn get_run(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::ReportingRead)?;
    let id = *path;

    let row = sqlx::query_as!(
        super::models::ReportRunRow,
        r#"
        SELECT id, scheduled_id, trigger_user_id, metric_ids,
               route_id, depot_id, date_from, date_to,
               granularity, output_format, status,
               result_data, error_message, started_at, completed_at, created_at
        FROM reporting.report_runs WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Run not found".to_string()))?;

    Ok(HttpResponse::Ok().json(ReportRunResponse::from(row)))
}

// ============================================================
// Export
// ============================================================

/// GET /reporting/runs/{id}/export?format=csv|pdf
/// Requires re-authentication within the last 10 minutes.
pub async fn export_run(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<Uuid>,
    query:   web::Query<ExportQuery>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::ReportingExport)?;
    let id = *path;

    let row = sqlx::query_as!(
        super::models::ReportRunRow,
        r#"
        SELECT id, scheduled_id, trigger_user_id, metric_ids,
               route_id, depot_id, date_from, date_to,
               granularity, output_format, status,
               result_data, error_message, started_at, completed_at, created_at
        FROM reporting.report_runs WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Run not found".to_string()))?;

    if row.status != "completed" {
        return Err(AppError::BadRequest(
            format!("run is not completed (status: {})", row.status),
        ));
    }

    let result_data = row.result_data.ok_or_else(|| {
        AppError::BadRequest("run has no result data".to_string())
    })?;

    let results: Vec<super::models::MetricResult> =
        serde_json::from_value(result_data).map_err(|e| {
            AppError::BadRequest(format!("result_data is malformed: {}", e))
        })?;

    let format       = query.format.clone().unwrap_or_else(|| row.output_format.clone());
    let viewer_name  = session.username.clone();
    let generated_at = Utc::now();

    crate::audit::writer::log(
        &state.db,
        &session,
        crate::audit::writer::AuditEntry {
            action:       crate::audit::writer::ACTION_EXPORT,
            domain:       "reporting",
            entity_type:  "report_run",
            entity_id:    id.to_string(),
            before_state: None,
            after_state:  None,
            metadata:     serde_json::json!({ "format": format }),
        },
    )
    .await
    .unwrap_or_else(|e| tracing::warn!(error = %e, "audit log failed"));

    match format.as_str() {
        "pdf" => {
            let bytes = export::to_pdf(&results, &viewer_name, generated_at)
                .map_err(AppError::Internal)?;
            Ok(HttpResponse::Ok()
                .content_type("application/pdf")
                .append_header((
                    "Content-Disposition",
                    format!("attachment; filename=\"report_{}.pdf\"", id),
                ))
                .body(bytes))
        }
        _ => {
            let bytes = export::to_csv(&results, &viewer_name, generated_at)
                .map_err(AppError::Internal)?;
            Ok(HttpResponse::Ok()
                .content_type("text/csv; charset=utf-8")
                .append_header((
                    "Content-Disposition",
                    format!("attachment; filename=\"report_{}.csv\"", id),
                ))
                .body(bytes))
        }
    }
}
