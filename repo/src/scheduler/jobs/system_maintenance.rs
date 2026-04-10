use std::time::Duration;
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::scheduler::{Job, JobError, JobOutcome};

use crate::reporting::scheduler as report_scheduler;

/// Consolidates 1-minute tasks (Config publish + Rollout activation + Report generation).
pub struct SystemMaintenanceJob;

#[async_trait]
impl Job for SystemMaintenanceJob {
    fn name(&self) -> &'static str { "system_maintenance_60s" }

    fn interval(&self) -> Duration { Duration::from_secs(60) }

    async fn run(&self, pool: &PgPool) -> Result<JobOutcome, JobError> {
        let published = auto_publish_configs(pool).await?;
        let activated = auto_activate_stages(pool).await?;
        
        // Report generation logic
        report_scheduler::tick(pool).await?;

        Ok(JobOutcome {
            summary: serde_json::json!({
                "configs_published":   published,
                "stages_activated":    activated,
                "reports_evaluated":   true,
            }),
        })
    }
}

// ── Config auto-publish ───────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ScheduledVersion {
    id:          Uuid,
    template_id: Uuid,
}

async fn auto_publish_configs(pool: &PgPool) -> Result<usize, sqlx::Error> {
    let due: Vec<ScheduledVersion> = sqlx::query_as(
        r#"
        SELECT id, template_id
        FROM   ops.config_versions
        WHERE  status        = 'scheduled'
          AND  effective_from <= now()
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_all(pool)
    .await?;

    let count = due.len();

    for v in due {
        if let Err(e) = publish_one(pool, v.template_id, v.id).await {
            tracing::error!(
                version_id  = %v.id,
                template_id = %v.template_id,
                error       = %e,
                "scheduled_config: auto-publish failed"
            );
        }
    }

    if count > 0 {
        tracing::info!(count, "scheduled_config: auto-published config versions");
    }
    Ok(count)
}

/// Identical transaction to `ops::config::publish_version`, minus session/audit context.
async fn publish_one(pool: &PgPool, template_id: Uuid, version_id: Uuid) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Archive existing published version
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

    // Publish target version
    sqlx::query(
        r#"
        UPDATE ops.config_versions
        SET    status       = 'published',
               published_at = now(),
               updated_at   = now()
        WHERE  id           = $1
          AND  template_id  = $2
          AND  status       = 'scheduled'
        "#,
    )
    .bind(version_id)
    .bind(template_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await
}

// ── Rollout stage auto-activation ────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct DueStage {
    id:           Uuid,
    plan_id:      Uuid,
    stage_number: i16,
    depot_ids:    Vec<Uuid>,
    // loaded via JOIN
    template_id:  Uuid,
    version_id:   Uuid,
    current_stage: i32,
}

async fn auto_activate_stages(pool: &PgPool) -> Result<usize, sqlx::Error> {
    // Only select stages that are the immediate next in their plan's sequence.
    let due: Vec<DueStage> = sqlx::query_as(
        r#"
        SELECT rs.id,
               rs.plan_id,
               rs.stage_number,
               rs.depot_ids,
               cv.template_id,
               rp.config_version_id  AS version_id,
               rp.current_stage
        FROM   ops.rollout_stages  rs
        JOIN   ops.rollout_plans   rp ON rp.id = rs.plan_id
        JOIN   ops.config_versions cv ON cv.id = rp.config_version_id
        WHERE  rs.status       = 'pending'
          AND  rs.scheduled_at IS NOT NULL
          AND  rs.scheduled_at <= now()
          AND  rs.stage_number  = rp.current_stage + 1
        FOR UPDATE OF rs SKIP LOCKED
        "#,
    )
    .fetch_all(pool)
    .await?;

    let count = due.len();

    for stage in due {
        if let Err(e) = activate_one(pool, &stage).await {
            tracing::error!(
                stage_id = %stage.id,
                plan_id  = %stage.plan_id,
                error    = %e,
                "scheduled_config: stage auto-activation failed"
            );
        }
    }

    if count > 0 {
        tracing::info!(count, "scheduled_config: auto-activated rollout stages");
    }
    Ok(count)
}

async fn activate_one(pool: &PgPool, stage: &DueStage) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Activate stage
    sqlx::query(
        r#"
        UPDATE ops.rollout_stages
        SET    status       = 'active',
               activated_at = now(),
               updated_at   = now()
        WHERE  id = $1
        "#,
    )
    .bind(stage.id)
    .execute(&mut *tx)
    .await?;

    // Upsert depot assignments
    for depot_id in &stage.depot_ids {
        sqlx::query(
            r#"
            INSERT INTO ops.depot_config_assignments
                (depot_id, template_id, config_version_id, rollout_stage_id, assigned_by)
            SELECT $1, $2, $3, $4, created_by
            FROM   ops.rollout_plans WHERE id = $5
            ON CONFLICT (depot_id, template_id) DO UPDATE
                SET config_version_id = EXCLUDED.config_version_id,
                    rollout_stage_id  = EXCLUDED.rollout_stage_id,
                    assigned_at       = now(),
                    assigned_by       = EXCLUDED.assigned_by
            "#,
        )
        .bind(depot_id)
        .bind(stage.template_id)
        .bind(stage.version_id)
        .bind(stage.id)
        .bind(stage.plan_id)
        .execute(&mut *tx)
        .await?;
    }

    // Advance plan counter
    let total_stages: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ops.rollout_stages WHERE plan_id = $1",
    )
    .bind(stage.plan_id)
    .fetch_one(&mut *tx)
    .await?;

    let new_stage = stage.current_stage + 1;
    let plan_status = if new_stage as i64 == total_stages {
        sqlx::query(
            "UPDATE ops.rollout_stages SET status = 'completed', updated_at = now() \
             WHERE plan_id = $1 AND status = 'active'",
        )
        .bind(stage.plan_id)
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
    .bind(stage.plan_id)
    .bind(new_stage)
    .bind(plan_status)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(
        stage_id     = %stage.id,
        plan_id      = %stage.plan_id,
        stage_number = stage.stage_number,
        depot_count  = stage.depot_ids.len(),
        plan_status,
        "Rollout stage auto-activated"
    );

    Ok(())
}
