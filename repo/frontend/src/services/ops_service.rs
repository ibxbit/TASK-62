use uuid::Uuid;

use crate::{
    services::api::{api_delete, api_get, api_post, api_post_empty, api_put},
    types::ops::{
        ChangePolicy, ConfigTemplate, ConfigVersion,
        CreateChangePolicyRequest, CreateFareRuleRequest,
        CreateRefundPolicyRequest, CreateRolloutRequest, CreateVersionRequest,
        CreateOpsCalendarRequest, CreateOpsRouteRequest, CreateOpsStopRequest,
        FareRule, OpsCalendar, OpsListPage, OpsRoute, OpsStop, RefundPolicy,
        UpdateOpsCalendarRequest, UpdateOpsRouteRequest, UpdateOpsStopRequest,
        RolloutPlan, RolloutStageSpec, Route, ScheduleVersionRequest,
        Trip, TripConflict, VersionDiff,
    },
};

// ── Config templates ──────────────────────────────────────────────────────────

pub async fn list_templates() -> Result<Vec<ConfigTemplate>, String> {
    api_get("/ops/configs").await
}

// ── Config versions ───────────────────────────────────────────────────────────

pub async fn list_versions(template_id: Uuid) -> Result<serde_json::Value, String> {
    api_get(&format!("/ops/configs/{}/versions", template_id)).await
}

pub async fn get_version(template_id: Uuid, version_id: Uuid) -> Result<ConfigVersion, String> {
    api_get(&format!("/ops/configs/{}/versions/{}", template_id, version_id)).await
}

pub async fn create_version(
    template_id: Uuid,
    body: &CreateVersionRequest,
) -> Result<ConfigVersion, String> {
    api_post(&format!("/ops/configs/{}/versions", template_id), body).await
}

pub async fn publish_version(
    template_id: Uuid,
    version_id: Uuid,
) -> Result<serde_json::Value, String> {
    api_post_empty(&format!(
        "/ops/configs/{}/versions/{}/publish",
        template_id, version_id
    ))
    .await
}

pub async fn unpublish_version(
    template_id: Uuid,
    version_id: Uuid,
) -> Result<serde_json::Value, String> {
    api_post_empty(&format!(
        "/ops/configs/{}/versions/{}/unpublish",
        template_id, version_id
    ))
    .await
}

pub async fn schedule_version(
    template_id: Uuid,
    version_id: Uuid,
    body: &ScheduleVersionRequest,
) -> Result<serde_json::Value, String> {
    api_post(
        &format!("/ops/configs/{}/versions/{}/schedule", template_id, version_id),
        body,
    )
    .await
}

pub async fn diff_versions(
    template_id: Uuid,
    v1: Uuid,
    v2: Uuid,
) -> Result<VersionDiff, String> {
    api_get(&format!(
        "/ops/configs/{}/versions/diff?v1={}&v2={}",
        template_id, v1, v2
    ))
    .await
}

pub async fn create_rollout(
    template_id: Uuid,
    version_id: Uuid,
    body: &CreateRolloutRequest,
) -> Result<RolloutPlan, String> {
    api_post(
        &format!("/ops/configs/{}/versions/{}/rollout", template_id, version_id),
        body,
    )
    .await
}

pub async fn get_rollout_plan(
    template_id: Uuid,
    plan_id: Uuid,
) -> Result<RolloutPlan, String> {
    api_get(&format!("/ops/configs/{}/rollout/{}", template_id, plan_id)).await
}

pub async fn activate_stage(
    template_id: Uuid,
    plan_id: Uuid,
    stage_id: Uuid,
) -> Result<serde_json::Value, String> {
    api_post_empty(&format!(
        "/ops/configs/{}/rollout/{}/stages/{}/activate",
        template_id, plan_id, stage_id
    ))
    .await
}

// ── Routes (dispatcher read-only) ────────────────────────────────────────────

pub async fn list_routes() -> Result<Vec<Route>, String> {
    api_get("/ops/routes").await
}

// ── Trips ─────────────────────────────────────────────────────────────────────

pub async fn list_trips() -> Result<Vec<Trip>, String> {
    api_get("/ops/trips").await
}

pub async fn list_conflicts() -> Result<Vec<TripConflict>, String> {
    api_get("/ops/conflicts").await
}

// ── Routes admin (full CRUD) ──────────────────────────────────────────────────

pub async fn list_routes_admin() -> Result<OpsListPage<OpsRoute>, String> {
    api_get("/ops/routes?per_page=100").await
}

pub async fn create_route_admin(body: &CreateOpsRouteRequest) -> Result<OpsRoute, String> {
    api_post("/ops/routes", body).await
}

pub async fn update_route_admin(
    route_id: Uuid,
    body: &UpdateOpsRouteRequest,
) -> Result<OpsRoute, String> {
    api_put(&format!("/ops/routes/{}", route_id), body).await
}

pub async fn delete_route_admin(route_id: Uuid) -> Result<(), String> {
    api_delete(&format!("/ops/routes/{}", route_id)).await
}

// ── Stops admin (nested under routes) ────────────────────────────────────────

pub async fn list_stops(route_id: Uuid) -> Result<Vec<OpsStop>, String> {
    api_get(&format!("/ops/routes/{}/stops", route_id)).await
}

pub async fn create_stop(
    route_id: Uuid,
    body: &CreateOpsStopRequest,
) -> Result<OpsStop, String> {
    api_post(&format!("/ops/routes/{}/stops", route_id), body).await
}

pub async fn update_stop(
    route_id: Uuid,
    stop_id: Uuid,
    body: &UpdateOpsStopRequest,
) -> Result<OpsStop, String> {
    api_put(&format!("/ops/routes/{}/stops/{}", route_id, stop_id), body).await
}

pub async fn delete_stop(route_id: Uuid, stop_id: Uuid) -> Result<(), String> {
    api_delete(&format!("/ops/routes/{}/stops/{}", route_id, stop_id)).await
}

// ── Calendars admin ───────────────────────────────────────────────────────────

pub async fn list_calendars() -> Result<Vec<OpsCalendar>, String> {
    api_get("/ops/calendars").await
}

pub async fn create_calendar(body: &CreateOpsCalendarRequest) -> Result<OpsCalendar, String> {
    api_post("/ops/calendars", body).await
}

pub async fn update_calendar(
    cal_id: Uuid,
    body: &UpdateOpsCalendarRequest,
) -> Result<OpsCalendar, String> {
    api_put(&format!("/ops/calendars/{}", cal_id), body).await
}

pub async fn delete_calendar(cal_id: Uuid) -> Result<(), String> {
    api_delete(&format!("/ops/calendars/{}", cal_id)).await
}

// ── Fare rules admin ──────────────────────────────────────────────────────────

pub async fn list_fare_rules() -> Result<Vec<FareRule>, String> {
    api_get("/ops/fare-rules").await
}

pub async fn create_fare_rule(body: &CreateFareRuleRequest) -> Result<FareRule, String> {
    api_post("/ops/fare-rules", body).await
}

pub async fn delete_fare_rule(rule_id: Uuid) -> Result<(), String> {
    api_delete(&format!("/ops/fare-rules/{}", rule_id)).await
}

// ── Change policies admin ─────────────────────────────────────────────────────

pub async fn list_change_policies() -> Result<Vec<ChangePolicy>, String> {
    api_get("/ops/change-policies").await
}

pub async fn create_change_policy(body: &CreateChangePolicyRequest) -> Result<ChangePolicy, String> {
    api_post("/ops/change-policies", body).await
}

pub async fn delete_change_policy(policy_id: Uuid) -> Result<(), String> {
    api_delete(&format!("/ops/change-policies/{}", policy_id)).await
}

// ── Refund policies admin ─────────────────────────────────────────────────────

pub async fn list_refund_policies() -> Result<Vec<RefundPolicy>, String> {
    api_get("/ops/refund-policies").await
}

pub async fn create_refund_policy(body: &CreateRefundPolicyRequest) -> Result<RefundPolicy, String> {
    api_post("/ops/refund-policies", body).await
}

pub async fn delete_refund_policy(policy_id: Uuid) -> Result<(), String> {
    api_delete(&format!("/ops/refund-policies/{}", policy_id)).await
}
