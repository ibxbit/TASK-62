//! API-service layer behaviour tests.
//!
//! Verifies the request-side contracts of the frontend service layer:
//!
//!   * Request bodies serialise to the exact field names the backend wants
//!     (renames on either side would fail these tests immediately).
//!   * Response bodies deserialise from the current backend shapes,
//!     including the `{data,page,per_page,total}` paged envelope.
//!   * `#[serde(skip_serializing_if = "Option::is_none")]` is honoured so the
//!     backend never sees a spurious `null` field.
//!
//! Framework evidence: `wasm_bindgen_test` in-browser harness.

use chrono::{TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;
use wasm_bindgen_test::*;

use transitops_frontend::types::alerting::{AlertRule, CreateAlertRuleRequest};
use transitops_frontend::types::auth::{LoginRequest, LoginResponse, ReauthRequest};
use transitops_frontend::types::notification::{Notification, ReceiptRequest};
use transitops_frontend::types::ops::{
    CreateOpsRouteRequest, CreateOpsStopRequest, OpsListPage, OpsRoute, ScheduleVersionRequest,
};
use transitops_frontend::types::reporting::{CreateMetricRequest, ReportRun};

wasm_bindgen_test_configure!(run_in_browser);

// ── Auth service wire contract ──────────────────────────────────────────────

#[wasm_bindgen_test]
fn login_request_serialises_to_expected_fields() {
    let req = LoginRequest {
        username: "admin".into(),
        password: "AdminPass123!".into(),
    };
    let v = serde_json::to_value(&req).unwrap();
    assert_eq!(v["username"], "admin");
    assert_eq!(v["password"], "AdminPass123!");
    let obj = v.as_object().unwrap();
    assert_eq!(obj.len(), 2, "LoginRequest must have exactly username+password");
}

#[wasm_bindgen_test]
fn reauth_request_has_only_password_field() {
    let req = ReauthRequest { password: "x".into() };
    let v = serde_json::to_value(&req).unwrap();
    assert_eq!(v["password"], "x");
    assert_eq!(v.as_object().unwrap().len(), 1);
}

#[wasm_bindgen_test]
fn login_response_parses_fields_used_by_auth_store() {
    let raw = json!({
        "token":    "abc",
        "username": "admin",
        "role":     "operations_admin",
    });
    let parsed: LoginResponse = serde_json::from_value(raw).unwrap();
    assert_eq!(parsed.token, "abc");
    assert_eq!(parsed.role, "operations_admin");
    assert_eq!(parsed.username, "admin");
}

// ── Ops service wire contract ───────────────────────────────────────────────

#[wasm_bindgen_test]
fn create_ops_route_request_serialises_code_and_name() {
    let req = CreateOpsRouteRequest {
        code: "R001".into(),
        name: "Route 1".into(),
        description: Some("x".into()),
        effective_from: None,
    };
    let v = serde_json::to_value(&req).unwrap();
    assert_eq!(v["code"], "R001");
    assert_eq!(v["name"], "Route 1");
    assert_eq!(v["description"], "x");
    // `effective_from` was None → must be omitted entirely, not sent as `null`.
    assert!(!v.as_object().unwrap().contains_key("effective_from"));
}

#[wasm_bindgen_test]
fn create_ops_stop_request_uses_sequence_order_not_sequence() {
    // Backend requires `sequence_order` — a rename to `sequence` on either
    // side would break this assertion, catching the drift before shipping.
    let req = CreateOpsStopRequest {
        code: "S1".into(),
        name: "Main St".into(),
        sequence_order: 1,
        latitude: Some(40.0),
        longitude: Some(-74.0),
    };
    let v = serde_json::to_value(&req).unwrap();
    let obj = v.as_object().unwrap();
    assert!(obj.contains_key("sequence_order"));
    assert!(!obj.contains_key("sequence"));
    assert_eq!(v["sequence_order"], 1);
    assert_eq!(v["latitude"], 40.0);
}

#[wasm_bindgen_test]
fn create_ops_stop_request_omits_none_coordinates() {
    let req = CreateOpsStopRequest {
        code: "S2".into(),
        name: "No GPS".into(),
        sequence_order: 2,
        latitude: None,
        longitude: None,
    };
    let v = serde_json::to_value(&req).unwrap();
    let obj = v.as_object().unwrap();
    assert!(!obj.contains_key("latitude"));
    assert!(!obj.contains_key("longitude"));
}

#[wasm_bindgen_test]
fn ops_list_page_parses_paged_envelope() {
    let raw = json!({
        "data": [{
            "id":             "00000000-0000-0000-0000-000000000001",
            "code":           "R",
            "name":           "R",
            "description":    null,
            "status":         "draft",
            "effective_from": null,
            "version":        1,
            "created_at":     "2026-01-02T03:04:05Z",
            "updated_at":     "2026-01-02T03:04:05Z",
        }],
        "page":     1,
        "per_page": 20,
        "total":    1,
    });
    let parsed: OpsListPage<OpsRoute> = serde_json::from_value(raw).expect("paged envelope parse");
    assert_eq!(parsed.page, 1);
    assert_eq!(parsed.per_page, 20);
    assert_eq!(parsed.total, 1);
    assert_eq!(parsed.data.len(), 1);
    assert_eq!(parsed.data[0].code, "R");
}

#[wasm_bindgen_test]
fn schedule_version_request_emits_effective_from() {
    let req = ScheduleVersionRequest {
        effective_from: Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap(),
        effective_to: None,
    };
    let v = serde_json::to_value(&req).unwrap();
    assert!(v.as_object().unwrap().contains_key("effective_from"));
    // `effective_to` None → omitted.
    assert!(!v.as_object().unwrap().contains_key("effective_to"));
}

// ── Notification service wire contract ──────────────────────────────────────

#[wasm_bindgen_test]
fn receipt_request_serialises_delivery_ids_array() {
    let ids = vec![Uuid::nil()];
    let req = ReceiptRequest { delivery_ids: ids };
    let v = serde_json::to_value(&req).unwrap();
    assert!(v["delivery_ids"].is_array());
    assert_eq!(v["delivery_ids"].as_array().unwrap().len(), 1);
}

#[wasm_bindgen_test]
fn notification_deserialises_with_optional_fields_null() {
    let raw = json!({
        "id":               Uuid::nil(),
        "event_id":         Uuid::nil(),
        "event_type":       "x.y.z",
        "severity":         "info",
        "source_entity_id": null,
        "payload":          {},
        "status":           "delivered",
        "delivered_at":     "2026-01-02T03:04:05Z",
        "read_at":          null,
        "created_at":       "2026-01-02T03:04:05Z",
    });
    let n: Notification = serde_json::from_value(raw).unwrap();
    assert_eq!(n.severity, "info");
    assert!(n.read_at.is_none());
}

// ── Reporting service wire contract ─────────────────────────────────────────

#[wasm_bindgen_test]
fn create_metric_request_omits_none_description() {
    let req = CreateMetricRequest {
        metric_key: "k".into(),
        display_name: "K".into(),
        description: None,
        formula_type: "count".into(),
    };
    let v = serde_json::to_value(&req).unwrap();
    assert!(!v.as_object().unwrap().contains_key("description"));
    assert_eq!(v["metric_key"], "k");
    assert_eq!(v["formula_type"], "count");
}

#[wasm_bindgen_test]
fn report_run_deserialises_with_completed_helper() {
    let raw = json!({
        "id":            "00000000-0000-0000-0000-000000000010",
        "scheduled_id":  null,
        "status":        "completed",
        "date_from":     "2026-01-01T00:00:00Z",
        "date_to":       "2026-02-01T00:00:00Z",
        "output_format": "csv",
        "result_data":   null,
        "error_message": null,
        "started_at":    "2026-02-01T00:05:00Z",
        "completed_at":  "2026-02-01T00:10:00Z",
        "created_at":    "2026-02-01T00:05:00Z",
    });
    let run: ReportRun = serde_json::from_value(raw).unwrap();
    assert!(run.is_completed());
    assert_eq!(run.output_format, "csv");
}

// ── Alerting service wire contract ──────────────────────────────────────────

#[wasm_bindgen_test]
fn create_alert_rule_round_trips_conditions_field() {
    let req = CreateAlertRuleRequest {
        name: "spike".into(),
        rule_type: "kpi_threshold".into(),
        severity: "warning".into(),
        conditions: json!({"metric_key": "on_time_departure_rate", "threshold": 0.9}),
        duplicate_suppression_window_secs: Some(300),
    };
    let v = serde_json::to_value(&req).unwrap();
    assert_eq!(v["severity"], "warning");
    assert_eq!(v["conditions"]["threshold"], 0.9);
    assert_eq!(v["duplicate_suppression_window_secs"], 300);
}

#[wasm_bindgen_test]
fn create_alert_rule_omits_none_suppression_window() {
    let req = CreateAlertRuleRequest {
        name: "n".into(),
        rule_type: "kpi_threshold".into(),
        severity: "info".into(),
        conditions: json!({}),
        duplicate_suppression_window_secs: None,
    };
    let v = serde_json::to_value(&req).unwrap();
    assert!(!v.as_object().unwrap().contains_key("duplicate_suppression_window_secs"));
}

#[wasm_bindgen_test]
fn alert_rule_parses_backend_row() {
    let raw = json!({
        "id":                                Uuid::nil(),
        "name":                              "spike",
        "rule_type":                         "kpi_threshold",
        "severity":                          "warning",
        "conditions":                        {"threshold": 0.9},
        "duplicate_suppression_window_secs": 300,
        "is_active":                         true,
        "created_at":                        "2026-01-02T03:04:05Z",
        "updated_at":                        "2026-01-02T03:04:05Z",
    });
    let rule: AlertRule = serde_json::from_value(raw).unwrap();
    assert_eq!(rule.name, "spike");
    assert_eq!(rule.severity, "warning");
    assert_eq!(rule.duplicate_suppression_window_secs, 300);
    assert!(rule.is_active);
}
