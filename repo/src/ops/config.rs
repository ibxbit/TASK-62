/// Config version management: draft/publish/unpublish/schedule, diff, and
/// gradual rollout by depot.
///
/// Versioning strategy:
///   Each `config_template` has an ordered list of versions.
///   version_number is monotonically increasing per template.
///   Only one version may be `published` at a time (enforced by partial unique index).
///   Status transitions:
///     draft ──publish──▶ published ──unpublish──▶ draft
///     draft ──schedule──▶ scheduled ──(time/job)──▶ published
///     published ──rollout──▶ gradual activation per depot stage
///     Any ──archive──▶ archived (via admin endpoint, not exposed here)
///
/// Gradual rollout:
///   1. POST /rollout  → creates a RolloutPlan with N stages (10%, 50%, 100%)
///   2. POST /rollout/{plan_id}/stages/{stage_id}/activate
///      → marks stage as 'active', upserts depot_config_assignments for that stage's depots
use actix_web::{web, HttpResponse};
use uuid::Uuid;

use crate::{
    auth::middleware::{AuthSession, ReauthGuard},
    error::AppError,
    ops::{
        diff::diff_versions,
        models::{
            ConfigVersionResponse, ConfigVersionRow, CreateConfigVersionRequest,
            CreateRolloutRequest, DiffQuery, ListResponse, OkResponse, RolloutPlanResponse,
            RolloutStageResponse, RolloutStageRow, ScheduleConfigRequest,
            UpdateConfigVersionRequest,
        },
    },
    rbac::permissions::Permission,
    AppState,
};

// ============================================================
// Config version CRUD
// ============================================================

/// GET /ops/configs/{template_id}/versions
pub async fn list_versions(
    state:       web::Data<AppState>,
    session:     AuthSession,
    path:        web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigRead)?;

    let template_id = path.into_inner();

    let rows = sqlx::query_as::<_, ConfigVersionRow>(
        r#"
        SELECT cv.id, cv.template_id, ct.key AS template_key,
               cv.version_number, cv.status, cv.payload,
               cv.effective_from, cv.effective_to,
               cv.published_at, cv.scheduled_at,
               cv.created_at, cv.updated_at
        FROM   ops.config_versions cv
        JOIN   ops.config_templates ct ON ct.id = cv.template_id
        WHERE  cv.template_id = $1
        ORDER  BY cv.version_number DESC
        "#,
    )
    .bind(template_id)
    .fetch_all(&state.db)
    .await?;

    let total = rows.len() as i64;
    let data: Vec<ConfigVersionResponse> = rows.into_iter().map(Into::into).collect();

    Ok(HttpResponse::Ok().json(ListResponse {
        data,
        total,
        page:     1,
        per_page: total,
    }))
}

/// POST /ops/configs/{template_id}/versions
/// Creates a new draft version. If `based_on_version` is set, copies its payload as a starting point.
pub async fn create_version(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    body:    web::Json<CreateConfigVersionRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigWrite)?;

    let template_id = path.into_inner();

    // Verify template exists
    let _template_key: Option<String> = sqlx::query_scalar(
        "SELECT key FROM ops.config_templates WHERE id = $1",
    )
    .bind(template_id)
    .fetch_optional(&state.db)
    .await?;

    if _template_key.is_none() {
        return Err(AppError::NotFound(format!("Template {} not found", template_id)));
    }

    // Determine base payload
    let payload = if let Some(base_id) = body.based_on_version {
        let base: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT payload FROM ops.config_versions WHERE id = $1 AND template_id = $2",
        )
        .bind(base_id)
        .bind(template_id)
        .fetch_optional(&state.db)
        .await?;

        base.map(|mut base_payload| {
            // Merge caller's payload on top of base
            if let (serde_json::Value::Object(base_map), serde_json::Value::Object(override_map)) =
                (&mut base_payload, &body.payload)
            {
                for (k, v) in override_map {
                    base_map.insert(k.clone(), v.clone());
                }
            }
            base_payload
        })
        .unwrap_or_else(|| body.payload.clone())
    } else {
        body.payload.clone()
    };

    // Next version number for this template
    let next_version: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM ops.config_versions WHERE template_id = $1",
    )
    .bind(template_id)
    .fetch_one(&state.db)
    .await?;

    let row = sqlx::query_as::<_, ConfigVersionRow>(
        r#"
        INSERT INTO ops.config_versions
            (template_id, version_number, status, payload, created_by)
        VALUES ($1, $2, 'draft', $3, $4)
        RETURNING id, template_id,
                  (SELECT key FROM ops.config_templates WHERE id = $1) AS template_key,
                  version_number, status, payload,
                  effective_from, effective_to,
                  published_at, scheduled_at,
                  created_at, updated_at
        "#,
    )
    .bind(template_id)
    .bind(next_version)
    .bind(payload)
    .bind(session.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(HttpResponse::Created().json(ConfigVersionResponse::from(row)))
}

/// GET /ops/configs/{template_id}/versions/{version_id}
pub async fn get_version(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigRead)?;

    let (template_id, version_id) = path.into_inner();

    let row = fetch_version_row(&state, template_id, version_id).await?;
    Ok(HttpResponse::Ok().json(ConfigVersionResponse::from(row)))
}

/// PUT /ops/configs/{template_id}/versions/{version_id}
/// Only draft versions can be updated.
pub async fn update_version(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<(Uuid, Uuid)>,
    body:    web::Json<UpdateConfigVersionRequest>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigWrite)?;

    let (template_id, version_id) = path.into_inner();

    let row = sqlx::query_as::<_, ConfigVersionRow>(
        r#"
        UPDATE ops.config_versions
        SET    payload    = $3,
               updated_at = now()
        WHERE  id          = $2
          AND  template_id = $1
          AND  status       = 'draft'
        RETURNING id, template_id,
                  (SELECT key FROM ops.config_templates WHERE id = $1) AS template_key,
                  version_number, status, payload,
                  effective_from, effective_to,
                  published_at, scheduled_at,
                  created_at, updated_at
        "#,
    )
    .bind(template_id)
    .bind(version_id)
    .bind(&body.payload)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| {
        AppError::NotFound("Version not found or is not in draft status".to_string())
    })?;

    Ok(HttpResponse::Ok().json(ConfigVersionResponse::from(row)))
}

// ============================================================
// Publish / Unpublish / Schedule
// ============================================================

/// POST /ops/configs/{template_id}/versions/{version_id}/publish
/// Transitions draft → published. Atomically moves any currently-published
/// version for this template to 'archived'.
/// Requires re-authentication within the last 10 minutes.
pub async fn publish_version(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::OpsConfigPublish)?;

    let (template_id, version_id) = path.into_inner();

    let mut tx = state.db.begin().await?;

    // Archive current published version (if any)
    sqlx::query(
        r#"
        UPDATE ops.config_versions
        SET    status = 'archived', updated_at = now()
        WHERE  template_id = $1 AND status = 'published'
        "#,
    )
    .bind(template_id)
    .execute(&mut *tx)
    .await?;

    // Publish the target version
    let affected = sqlx::query(
        r#"
        UPDATE ops.config_versions
        SET    status       = 'published',
               published_at = now(),
               updated_at   = now()
        WHERE  id           = $1
          AND  template_id  = $2
          AND  status       IN ('draft', 'scheduled')
        "#,
    )
    .bind(version_id)
    .bind(template_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if affected == 0 {
        tx.rollback().await?;
        return Err(AppError::BadRequest(
            "Version not found or is not in draft/scheduled status".to_string(),
        ));
    }

    tx.commit().await?;

    crate::audit::writer::log(
        &state.db,
        &session,
        crate::audit::writer::AuditEntry {
            action:       crate::audit::writer::ACTION_CONFIG_PUBLISH,
            domain:       "ops",
            entity_type:  "config_version",
            entity_id:    version_id.to_string(),
            before_state: None,
            after_state:  None,
            metadata:     serde_json::json!({ "template_id": template_id }),
        },
    )
    .await
    .unwrap_or_else(|e| tracing::warn!(error = %e, "audit log failed"));

    tracing::info!(
        template_id = %template_id,
        version_id  = %version_id,
        user_id     = %session.user_id,
        "Config version published"
    );

    Ok(HttpResponse::Ok().json(OkResponse { message: "Version published" }))
}

/// POST /ops/configs/{template_id}/versions/{version_id}/unpublish
/// published → draft.  Does NOT restore any previously-archived version.
/// Requires re-authentication within the last 10 minutes.
pub async fn unpublish_version(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::OpsConfigPublish)?;

    let (template_id, version_id) = path.into_inner();

    let affected = sqlx::query(
        r#"
        UPDATE ops.config_versions
        SET    status      = 'draft',
               published_at = NULL,
               updated_at  = now()
        WHERE  id           = $1
          AND  template_id  = $2
          AND  status       = 'published'
        "#,
    )
    .bind(version_id)
    .bind(template_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::BadRequest(
            "Version not found or is not currently published".to_string(),
        ));
    }

    crate::audit::writer::log(
        &state.db,
        &session,
        crate::audit::writer::AuditEntry {
            action:       crate::audit::writer::ACTION_CONFIG_UNPUBLISH,
            domain:       "ops",
            entity_type:  "config_version",
            entity_id:    version_id.to_string(),
            before_state: None,
            after_state:  None,
            metadata:     serde_json::json!({ "template_id": template_id }),
        },
    )
    .await
    .unwrap_or_else(|e| tracing::warn!(error = %e, "audit log failed"));

    Ok(HttpResponse::Ok().json(OkResponse { message: "Version unpublished" }))
}

/// POST /ops/configs/{template_id}/versions/{version_id}/schedule
/// Sets status = 'scheduled' with a future effective_from.
/// A background scheduler (e.g. pg_cron or app-level cron) calls `publish_version`
/// when `effective_from` is reached.
/// Requires re-authentication within the last 10 minutes.
pub async fn schedule_version(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<(Uuid, Uuid)>,
    body:    web::Json<ScheduleConfigRequest>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::OpsConfigPublish)?;

    let (template_id, version_id) = path.into_inner();

    if body.effective_from <= chrono::Utc::now() {
        return Err(AppError::BadRequest(
            "effective_from must be in the future".to_string(),
        ));
    }

    if let Some(to) = body.effective_to {
        if to <= body.effective_from {
            return Err(AppError::BadRequest(
                "effective_to must be after effective_from".to_string(),
            ));
        }
    }

    let affected = sqlx::query(
        r#"
        UPDATE ops.config_versions
        SET    status         = 'scheduled',
               scheduled_at  = $3,
               effective_from = $3,
               effective_to   = $4,
               updated_at     = now()
        WHERE  id             = $1
          AND  template_id    = $2
          AND  status         = 'draft'
        "#,
    )
    .bind(version_id)
    .bind(template_id)
    .bind(body.effective_from)
    .bind(body.effective_to)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::BadRequest(
            "Version not found or is not in draft status".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message":      "Version scheduled for publication",
        "effective_from": body.effective_from,
        "effective_to":   body.effective_to
    })))
}

// ============================================================
// Diff
// ============================================================

/// GET /ops/configs/{template_id}/versions/diff?v1={uuid}&v2={uuid}
///
/// Returns a structural diff between two version payloads.
/// Both versions must belong to the same template_id.
pub async fn diff_config_versions(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<Uuid>,
    query:   web::Query<DiffQuery>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigRead)?;

    let template_id = path.into_inner();

    if query.v1 == query.v2 {
        return Err(AppError::BadRequest("v1 and v2 must be different versions".to_string()));
    }

    // Load both versions in a single query; validate they belong to this template
    let rows = sqlx::query_as::<_, ConfigVersionRow>(
        r#"
        SELECT cv.id, cv.template_id, ct.key AS template_key,
               cv.version_number, cv.status, cv.payload,
               cv.effective_from, cv.effective_to,
               cv.published_at, cv.scheduled_at,
               cv.created_at, cv.updated_at
        FROM   ops.config_versions cv
        JOIN   ops.config_templates ct ON ct.id = cv.template_id
        WHERE  cv.id IN ($1, $2) AND cv.template_id = $3
        ORDER  BY cv.version_number ASC
        "#,
    )
    .bind(query.v1)
    .bind(query.v2)
    .bind(template_id)
    .fetch_all(&state.db)
    .await?;

    if rows.len() < 2 {
        return Err(AppError::BadRequest(
            "One or both versions not found for this template".to_string(),
        ));
    }

    // Determine which is older/newer by version_number
    let (old, new) = if rows[0].version_number <= rows[1].version_number {
        (&rows[0], &rows[1])
    } else {
        (&rows[1], &rows[0])
    };

    let diff = diff_versions(
        &old.payload,
        &new.payload,
        old.version_number.to_string(),
        new.version_number.to_string(),
    );

    Ok(HttpResponse::Ok().json(diff))
}

// ============================================================
// Gradual rollout
// ============================================================

/// POST /ops/configs/{template_id}/versions/{version_id}/rollout
///
/// Creates a rollout plan with ordered stages.  The config_version must be
/// 'published'.  Stages should cover 100% of depots collectively.
/// Requires re-authentication within the last 10 minutes.
pub async fn create_rollout(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<(Uuid, Uuid)>,
    body:    web::Json<CreateRolloutRequest>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::OpsConfigPublish)?;

    let (template_id, version_id) = path.into_inner();

    if body.stages.is_empty() {
        return Err(AppError::BadRequest("At least one stage is required".to_string()));
    }

    // Validate stages are in increasing percentage order and depot lists non-empty
    let mut last_pct: i16 = 0;
    for (i, stage) in body.stages.iter().enumerate() {
        if stage.depot_ids.is_empty() {
            return Err(AppError::BadRequest(format!(
                "Stage {} has no depot_ids",
                i + 1
            )));
        }
        if stage.target_percentage <= last_pct {
            return Err(AppError::BadRequest(
                "Stages must have strictly increasing target_percentage values".to_string(),
            ));
        }
        last_pct = stage.target_percentage;
    }

    // Verify version is published and belongs to the template
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM ops.config_versions WHERE id = $1 AND template_id = $2",
    )
    .bind(version_id)
    .bind(template_id)
    .fetch_optional(&state.db)
    .await?;

    match status.as_deref() {
        Some("published") => {}
        Some(s) => {
            return Err(AppError::BadRequest(format!(
                "Config version must be published before creating a rollout (current: {})",
                s
            )))
        }
        None => {
            return Err(AppError::NotFound(
                "Config version not found for this template".to_string(),
            ))
        }
    }

    let total_depots: i32 = body
        .stages
        .iter()
        .map(|s| s.depot_ids.len() as i32)
        .sum();

    let mut tx = state.db.begin().await?;

    // Create plan
    let plan_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO ops.rollout_plans
            (config_version_id, status, total_depots, current_stage, created_by, notes)
        VALUES ($1, 'pending', $2, 0, $3, $4)
        RETURNING id
        "#,
    )
    .bind(version_id)
    .bind(total_depots)
    .bind(session.user_id)
    .bind(body.notes.as_deref())
    .fetch_one(&mut *tx)
    .await?;

    // Create stages
    let mut stage_responses = Vec::new();
    for (i, spec) in body.stages.iter().enumerate() {
        let stage_number = (i + 1) as i16;
        let stage_row = sqlx::query_as::<_, RolloutStageRow>(
            r#"
            INSERT INTO ops.rollout_stages
                (plan_id, stage_number, target_percentage, depot_ids, status, scheduled_at)
            VALUES ($1, $2, $3, $4, 'pending', $5)
            RETURNING id, stage_number, target_percentage, depot_ids,
                      status, scheduled_at, activated_at
            "#,
        )
        .bind(plan_id)
        .bind(stage_number)
        .bind(spec.target_percentage)
        .bind(&spec.depot_ids)
        .bind(spec.scheduled_at)
        .fetch_one(&mut *tx)
        .await?;

        stage_responses.push(RolloutStageResponse {
            id:                stage_row.id,
            stage_number:      stage_row.stage_number,
            target_percentage: stage_row.target_percentage,
            depot_count:       stage_row.depot_ids.len(),
            status:            stage_row.status,
            scheduled_at:      stage_row.scheduled_at,
            activated_at:      stage_row.activated_at,
        });
    }

    tx.commit().await?;

    tracing::info!(
        plan_id    = %plan_id,
        version_id = %version_id,
        stages     = body.stages.len(),
        "Rollout plan created"
    );

    Ok(HttpResponse::Created().json(RolloutPlanResponse {
        id:                plan_id,
        config_version_id: version_id,
        status:            "pending".to_string(),
        total_depots,
        current_stage:     0,
        stages:            stage_responses,
        created_at:        chrono::Utc::now(),
    }))
}

/// GET /ops/configs/{template_id}/rollout/{plan_id}
///
/// Returns a rollout plan with all its stages.
/// Requires OpsConfigRead permission.
pub async fn get_rollout_plan(
    state:   web::Data<AppState>,
    session: AuthSession,
    path:    web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, AppError> {
    session.require(Permission::OpsConfigRead)?;

    let (template_id, plan_id) = path.into_inner();

    // Verify plan ownership: plan's config_version must belong to this template
    #[derive(sqlx::FromRow)]
    struct PlanRow {
        id:                uuid::Uuid,
        config_version_id: uuid::Uuid,
        status:            String,
        total_depots:      i32,
        current_stage:     i32,
        created_at:        chrono::DateTime<chrono::Utc>,
    }

    let plan = sqlx::query_as::<_, PlanRow>(
        r#"
        SELECT rp.id, rp.config_version_id, rp.status,
               rp.total_depots, rp.current_stage, rp.created_at
        FROM   ops.rollout_plans  rp
        JOIN   ops.config_versions cv ON cv.id = rp.config_version_id
        WHERE  rp.id = $1 AND cv.template_id = $2
        "#,
    )
    .bind(plan_id)
    .bind(template_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Rollout plan {} not found for this template", plan_id)))?;

    let stage_rows = sqlx::query_as::<_, RolloutStageRow>(
        r#"
        SELECT id, stage_number, target_percentage, depot_ids,
               status, scheduled_at, activated_at
        FROM   ops.rollout_stages
        WHERE  plan_id = $1
        ORDER  BY stage_number ASC
        "#,
    )
    .bind(plan_id)
    .fetch_all(&state.db)
    .await?;

    let stages: Vec<RolloutStageResponse> = stage_rows
        .into_iter()
        .map(|s| RolloutStageResponse {
            id:                s.id,
            stage_number:      s.stage_number,
            target_percentage: s.target_percentage,
            depot_count:       s.depot_ids.len(),
            status:            s.status,
            scheduled_at:      s.scheduled_at,
            activated_at:      s.activated_at,
        })
        .collect();

    Ok(HttpResponse::Ok().json(RolloutPlanResponse {
        id:                plan.id,
        config_version_id: plan.config_version_id,
        status:            plan.status,
        total_depots:      plan.total_depots,
        current_stage:     plan.current_stage,
        stages,
        created_at:        plan.created_at,
    }))
}

/// POST /ops/configs/{template_id}/rollout/{plan_id}/stages/{stage_id}/activate
///
/// Activates a rollout stage:
///   1. Marks the stage as 'active'
///   2. Upserts depot_config_assignments for every depot in this stage
///   3. Advances rollout_plan.current_stage
///   4. If all stages complete, marks plan as 'completed'
/// Requires re-authentication within the last 10 minutes.
pub async fn activate_rollout_stage(
    state:   web::Data<AppState>,
    session: ReauthGuard,
    path:    web::Path<(Uuid, Uuid, Uuid)>,
) -> Result<HttpResponse, AppError> {
    let session = session.0;
    session.require(Permission::OpsConfigPublish)?;

    let (template_id, plan_id, stage_id) = path.into_inner();

    // Load stage (validates plan ownership)
    let stage = sqlx::query_as::<_, RolloutStageRow>(
        r#"
        SELECT rs.id, rs.stage_number, rs.target_percentage,
               rs.depot_ids, rs.status, rs.scheduled_at, rs.activated_at
        FROM   ops.rollout_stages rs
        JOIN   ops.rollout_plans  rp ON rp.id = rs.plan_id
        WHERE  rs.id = $1 AND rs.plan_id = $2 AND rp.config_version_id IN (
            SELECT id FROM ops.config_versions WHERE template_id = $3
        )
        "#,
    )
    .bind(stage_id)
    .bind(plan_id)
    .bind(template_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Stage not found for this plan/template".to_string()))?;

    if stage.status != "pending" {
        return Err(AppError::BadRequest(format!(
            "Stage is already {}",
            stage.status
        )));
    }

    // Load config_version_id from the plan
    let (config_version_id, previous_stage): (Uuid, i32) = sqlx::query_as(
        "SELECT config_version_id, current_stage FROM ops.rollout_plans WHERE id = $1",
    )
    .bind(plan_id)
    .fetch_one(&state.db)
    .await?;

    // Enforce sequential stage activation
    if stage.stage_number as i32 != previous_stage + 1 {
        return Err(AppError::BadRequest(format!(
            "Must activate stage {} before stage {}",
            previous_stage + 1,
            stage.stage_number
        )));
    }

    let mut tx = state.db.begin().await?;

    // Activate stage
    sqlx::query(
        r#"
        UPDATE ops.rollout_stages
        SET    status       = 'active',
               activated_at = now(),
               activated_by = $2,
               updated_at   = now()
        WHERE  id = $1
        "#,
    )
    .bind(stage_id)
    .bind(session.user_id)
    .execute(&mut *tx)
    .await?;

    // Upsert depot_config_assignments for every depot in this stage
    for depot_id in &stage.depot_ids {
        sqlx::query(
            r#"
            INSERT INTO ops.depot_config_assignments
                (depot_id, template_id, config_version_id, rollout_stage_id, assigned_by)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (depot_id, template_id) DO UPDATE
                SET config_version_id = EXCLUDED.config_version_id,
                    rollout_stage_id  = EXCLUDED.rollout_stage_id,
                    assigned_at       = now(),
                    assigned_by       = EXCLUDED.assigned_by
            "#,
        )
        .bind(depot_id)
        .bind(template_id)
        .bind(config_version_id)
        .bind(stage_id)
        .bind(session.user_id)
        .execute(&mut *tx)
        .await?;
    }

    // Advance plan counter; check if all stages are now active → mark plan completed
    let total_stages: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ops.rollout_stages WHERE plan_id = $1",
    )
    .bind(plan_id)
    .fetch_one(&mut *tx)
    .await?;

    let new_stage = previous_stage + 1;
    let plan_status = if new_stage as i64 == total_stages {
        // Mark all stages completed
        sqlx::query(
            "UPDATE ops.rollout_stages SET status = 'completed', updated_at = now() WHERE plan_id = $1 AND status = 'active'",
        )
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;
        "completed"
    } else {
        "active"
    };

    sqlx::query(
        r#"
        UPDATE ops.rollout_plans
        SET    current_stage = $2, status = $3, updated_at = now()
        WHERE  id = $1
        "#,
    )
    .bind(plan_id)
    .bind(new_stage)
    .bind(plan_status)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(
        plan_id      = %plan_id,
        stage_id     = %stage_id,
        stage_number = %stage.stage_number,
        depot_count  = stage.depot_ids.len(),
        "Rollout stage activated"
    );

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message":       "Stage activated",
        "stage_number":  stage.stage_number,
        "depots_updated": stage.depot_ids.len(),
        "plan_status":   plan_status
    })))
}

// ============================================================
// Private helper
// ============================================================

async fn fetch_version_row(
    state:       &AppState,
    template_id: Uuid,
    version_id:  Uuid,
) -> Result<ConfigVersionRow, AppError> {
    sqlx::query_as::<_, ConfigVersionRow>(
        r#"
        SELECT cv.id, cv.template_id, ct.key AS template_key,
               cv.version_number, cv.status, cv.payload,
               cv.effective_from, cv.effective_to,
               cv.published_at, cv.scheduled_at,
               cv.created_at, cv.updated_at
        FROM   ops.config_versions cv
        JOIN   ops.config_templates ct ON ct.id = cv.template_id
        WHERE  cv.id = $1 AND cv.template_id = $2
        "#,
    )
    .bind(version_id)
    .bind(template_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Version {} not found", version_id)))
}
