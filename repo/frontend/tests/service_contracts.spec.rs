//! Service-layer contract tests.
//!
//! Covers the JSON serialisation/deserialisation contracts between the Yew
//! frontend's service types and the backend HTTP API.  A drift between the two
//! sides (e.g. a renamed field, a missing optional) would be caught here
//! before it reaches an E2E run.
//!
//! Also covers the `TOKEN_KEY` contract between `auth_store` and `api.rs` —
//! both modules must use the exact same localStorage key or auth breaks.

use serde_json::json;
use uuid::Uuid;
use wasm_bindgen_test::*;

use transitops_frontend::store::auth_store::TOKEN_KEY;
use transitops_frontend::types::auth::{LoginResponse, SessionInfo};
use transitops_frontend::types::notification::{Notification, UnreadCountResponse};
use transitops_frontend::types::reporting::MetricValue;

wasm_bindgen_test_configure!(run_in_browser);

// ── TOKEN_KEY contract ──────────────────────────────────────────────────────

#[wasm_bindgen_test]
fn token_key_is_the_documented_constant() {
    // The constant is load-bearing: services read it from localStorage and
    // the Playwright E2E suite asserts it for the post-login sanity check.
    assert_eq!(TOKEN_KEY, "transitops_token");
}

// ── LoginResponse contract ──────────────────────────────────────────────────

#[wasm_bindgen_test]
fn login_response_parses_backend_shape() {
    let raw = json!({
        "token": "abc123",
        "username": "admin",
        "role": "operations_admin"
    });
    let parsed: LoginResponse = serde_json::from_value(raw).expect("parse");
    assert_eq!(parsed.token, "abc123");
    assert_eq!(parsed.username, "admin");
    assert_eq!(parsed.role, "operations_admin");
}

#[wasm_bindgen_test]
fn login_response_tolerates_extra_backend_fields() {
    // Backend may add `expires_at`, `user_id`, etc. — must not break parsing.
    let raw = json!({
        "token": "t",
        "username": "admin",
        "role": "operations_admin",
        "expires_at": "2099-01-01T00:00:00Z",
        "user_id": "c0000001-0000-4000-8000-000000000001"
    });
    let parsed: Result<LoginResponse, _> = serde_json::from_value(raw);
    assert!(parsed.is_ok());
}

// ── SessionInfo contract ────────────────────────────────────────────────────

#[wasm_bindgen_test]
fn session_info_accepts_missing_session_id() {
    let raw = json!({"username": "admin", "role": "operations_admin"});
    let s: SessionInfo = serde_json::from_value(raw).expect("parse");
    assert!(s.is_admin());
    assert!(s.session_id.is_none());
}

#[wasm_bindgen_test]
fn session_info_role_helpers_are_mutually_exclusive() {
    let admin = SessionInfo {
        username: "a".into(),
        role: "operations_admin".into(),
        session_id: None,
    };
    let staff = SessionInfo {
        username: "s".into(),
        role: "staff_user".into(),
        session_id: None,
    };
    assert!(admin.is_admin() && !admin.is_staff());
    assert!(staff.is_staff() && !staff.is_admin());
    assert!(admin.can_publish());
    assert!(!staff.can_publish());
    // can_alerts: staff is the only one denied.
    assert!(admin.can_alerts());
    assert!(!staff.can_alerts());
}

// ── UnreadCountResponse contract ────────────────────────────────────────────

#[wasm_bindgen_test]
fn unread_count_parses_backend_shape() {
    let raw = json!({"unread": 3, "queued": 7});
    let parsed: UnreadCountResponse = serde_json::from_value(raw).expect("parse");
    assert_eq!(parsed.unread, 3);
    assert_eq!(parsed.queued, 7);
}

#[wasm_bindgen_test]
fn unread_count_default_is_zeroed() {
    let d = UnreadCountResponse::default();
    assert_eq!(d.unread, 0);
    assert_eq!(d.queued, 0);
}

// ── Notification wire contract ──────────────────────────────────────────────

#[wasm_bindgen_test]
fn notification_parses_full_inbox_row() {
    let raw = json!({
        "id":           "00000000-0000-0000-0000-000000000001",
        "event_id":     "00000000-0000-0000-0000-000000000002",
        "event_type":   "sys.announcement",
        "severity":     "info",
        "source_entity_id": null,
        "payload":      { "title": "Hello", "message": "body" },
        "status":       "delivered",
        "delivered_at": "2026-01-02T03:04:05Z",
        "read_at":      null,
        "created_at":   "2026-01-02T03:04:05Z"
    });
    let n: Notification = serde_json::from_value(raw).expect("parse");
    assert_eq!(n.title(), "Hello");
    assert_eq!(n.severity_class(), "info");
    assert!(n.is_unread());
}

#[wasm_bindgen_test]
fn notification_parses_with_null_delivered_at() {
    let raw = json!({
        "id":           Uuid::nil(),
        "event_id":     Uuid::nil(),
        "event_type":   "x.y.z",
        "severity":     "info",
        "source_entity_id": null,
        "payload":      {},
        "status":       "queued",
        "delivered_at": null,
        "read_at":      null,
        "created_at":   "2026-01-02T03:04:05Z"
    });
    let n: Notification = serde_json::from_value(raw).expect("parse");
    assert!(n.formatted_delivered_at().is_none());
    assert!(n.is_queued());
}

// ── MetricValue contract (reporting flow in E2E depends on this) ────────────

#[wasm_bindgen_test]
fn metric_value_parses_backend_row() {
    let raw = json!({
        "metric_id":    "00000000-0000-0000-0000-000000000010",
        "period_start": "2026-01-02T00:00:00Z",
        "period_end":   "2026-01-03T00:00:00Z",
        "value":        97.5,
        "dimensions":   {"route_id": "R1"}
    });
    let parsed: Result<MetricValue, _> = serde_json::from_value(raw);
    assert!(parsed.is_ok(), "{:?}", parsed.err());
    let mv = parsed.unwrap();
    assert!((mv.value - 97.5).abs() < 1e-9);
}
