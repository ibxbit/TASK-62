//! Notification domain-logic tests (pure, no browser features required).
//!
//! Covers the `Notification` helper methods and the `StatusFilter` enum which
//! drive the inbox UI rendering and API query parameters.  These tests were
//! identified by the test-coverage audit as missing focused coverage.

use chrono::{TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;
use wasm_bindgen_test::*;

use transitops_frontend::types::notification::{Notification, StatusFilter};

wasm_bindgen_test_configure!(run_in_browser);

fn make_notification(status: &str, severity: &str, payload: serde_json::Value) -> Notification {
    Notification {
        id:               Uuid::nil(),
        event_id:         Uuid::nil(),
        event_type:       "ops.trip.conflict_detected".to_string(),
        severity:         severity.to_string(),
        source_entity_id: None,
        payload,
        status:           status.to_string(),
        delivered_at:     Some(Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()),
        read_at:          None,
        created_at:       Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap(),
    }
}

#[wasm_bindgen_test]
fn is_unread_matches_delivered_status() {
    let n = make_notification("delivered", "info", json!({}));
    assert!(n.is_unread());
    assert!(!n.is_queued());
    assert!(!n.is_read());
}

#[wasm_bindgen_test]
fn is_queued_matches_queued_status() {
    let n = make_notification("queued", "info", json!({}));
    assert!(n.is_queued());
    assert!(!n.is_unread());
}

#[wasm_bindgen_test]
fn is_read_matches_read_status() {
    let n = make_notification("read", "info", json!({}));
    assert!(n.is_read());
}

#[wasm_bindgen_test]
fn title_prefers_payload_title_over_event_type() {
    let n = make_notification("delivered", "info", json!({"title": "Explicit title"}));
    assert_eq!(n.title(), "Explicit title");
}

#[wasm_bindgen_test]
fn title_falls_back_to_humanized_event_type() {
    let n = make_notification("delivered", "info", json!({}));
    // "ops.trip.conflict_detected" → "ops › trip › conflict detected"
    let title = n.title();
    assert!(title.contains("ops"));
    assert!(title.contains("conflict detected"));
    assert!(title.contains("›"));
}

#[wasm_bindgen_test]
fn message_prefers_message_over_description() {
    let n = make_notification(
        "delivered",
        "info",
        json!({"message": "msg body", "description": "should-not-use"}),
    );
    assert_eq!(n.message().as_deref(), Some("msg body"));
}

#[wasm_bindgen_test]
fn message_falls_back_to_description_when_no_message() {
    let n = make_notification("delivered", "info", json!({"description": "desc body"}));
    assert_eq!(n.message().as_deref(), Some("desc body"));
}

#[wasm_bindgen_test]
fn message_returns_none_when_neither_field_present() {
    let n = make_notification("delivered", "info", json!({"other": 1}));
    assert!(n.message().is_none());
}

#[wasm_bindgen_test]
fn severity_class_maps_all_known_severities() {
    assert_eq!(
        make_notification("delivered", "critical", json!({})).severity_class(),
        "critical"
    );
    assert_eq!(
        make_notification("delivered", "warning", json!({})).severity_class(),
        "warning"
    );
    // Anything else falls through to "info".
    assert_eq!(
        make_notification("delivered", "unknown", json!({})).severity_class(),
        "info"
    );
    assert_eq!(
        make_notification("delivered", "info", json!({})).severity_class(),
        "info"
    );
}

#[wasm_bindgen_test]
fn category_derives_from_event_type_prefix() {
    let mut n = make_notification("delivered", "info", json!({}));
    n.event_type = "ops.trip.started".into();
    assert_eq!(n.category(), "Trip");
    n.event_type = "ops.request.submitted".into();
    assert_eq!(n.category(), "Request");
    n.event_type = "sys.announcement".into();
    assert_eq!(n.category(), "System");
    n.event_type = "payments.captured".into();
    assert_eq!(n.category(), "Other");
}

#[wasm_bindgen_test]
fn status_filter_query_params_match_backend_contract() {
    assert_eq!(StatusFilter::Unread.as_query_param(), "unread");
    assert_eq!(StatusFilter::Queued.as_query_param(), "queued");
    assert_eq!(StatusFilter::All.as_query_param(), "all");
}

#[wasm_bindgen_test]
fn status_filter_default_is_unread() {
    assert!(matches!(StatusFilter::default(), StatusFilter::Unread));
}

#[wasm_bindgen_test]
fn status_filter_labels_are_human_readable() {
    assert_eq!(StatusFilter::Unread.label(), "Unread");
    assert_eq!(StatusFilter::All.label(), "All");
    // "Queued (DND held)" must explain that queued items are DND-deferred.
    let queued_label = StatusFilter::Queued.label();
    assert!(queued_label.to_lowercase().contains("dnd"));
}
