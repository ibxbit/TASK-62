use uuid::Uuid;

use crate::{
    services::api::{api_delete, api_get, api_post, api_post_empty, api_put},
    types::alerting::{Alert, AlertRule, AlertStats, CreateAlertRuleRequest, UpdateAlertRuleRequest},
};

// ── Alert list / actions ──────────────────────────────────────────────────────

pub async fn list_alerts(status: Option<&str>) -> Result<Vec<Alert>, String> {
    let path = match status {
        Some(s) => format!("/alerts?status={}", s),
        None    => "/alerts".to_string(),
    };
    api_get(&path).await
}

pub async fn get_stats() -> Result<AlertStats, String> {
    api_get("/alerts/stats").await
}

pub async fn acknowledge_alert(id: Uuid) -> Result<serde_json::Value, String> {
    api_post_empty(&format!("/alerts/{}/acknowledge", id)).await
}

pub async fn close_alert(id: Uuid) -> Result<serde_json::Value, String> {
    api_post_empty(&format!("/alerts/{}/close", id)).await
}

// ── Alert rule subscriptions ──────────────────────────────────────────────────

pub async fn list_rules() -> Result<Vec<AlertRule>, String> {
    api_get("/alerts/rules").await
}

pub async fn create_rule(body: &CreateAlertRuleRequest) -> Result<AlertRule, String> {
    api_post("/alerts/rules", body).await
}

pub async fn update_rule(rule_id: Uuid, body: &UpdateAlertRuleRequest) -> Result<AlertRule, String> {
    api_put(&format!("/alerts/rules/{}", rule_id), body).await
}

pub async fn delete_rule(rule_id: Uuid) -> Result<(), String> {
    api_delete(&format!("/alerts/rules/{}", rule_id)).await
}
