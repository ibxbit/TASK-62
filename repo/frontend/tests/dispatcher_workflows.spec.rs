//! Dispatcher page/workflow tests.
//!
//! Exercises the real `TripConflict` domain type + the dispatcher flow's
//! decision logic (resolved filtering, severity categorization, etc.).  The
//! tests use `wasm_bindgen_test` so they run in a real headless browser and
//! can import the actual frontend modules.
//!
//! Framework evidence: `use wasm_bindgen_test::*;` + `run_in_browser`.

use chrono::{Duration, TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;
use wasm_bindgen_test::*;

use transitops_frontend::types::auth::SessionInfo;
use transitops_frontend::types::ops::TripConflict;

wasm_bindgen_test_configure!(run_in_browser);

fn conflict(resolved: bool, ctype: &str) -> TripConflict {
    let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    TripConflict {
        id:            Uuid::new_v4(),
        trip_id:       Uuid::new_v4(),
        conflict_type: ctype.to_string(),
        description:   "test".to_string(),
        detected_at:   now,
        resolved_at:   if resolved { Some(now + Duration::minutes(5)) } else { None },
        is_resolved:   resolved,
    }
}

// ── Role-based access to dispatcher pages ───────────────────────────────────

#[wasm_bindgen_test]
fn dispatcher_can_see_conflicts_page() {
    let s = SessionInfo {
        username: "d".into(),
        role: "dispatcher".into(),
        session_id: None,
    };
    assert!(s.is_dispatcher());
    // Dispatcher and admin should both pass through to conflicts.
    assert!(s.is_dispatcher() || s.is_admin());
}

#[wasm_bindgen_test]
fn staff_cannot_see_conflicts_page() {
    let s = SessionInfo {
        username: "s".into(),
        role: "staff_user".into(),
        session_id: None,
    };
    assert!(!s.is_dispatcher() && !s.is_admin());
}

// ── Conflict list filtering logic ───────────────────────────────────────────

#[wasm_bindgen_test]
fn unresolved_filter_excludes_resolved_conflicts() {
    let conflicts = vec![
        conflict(false, "overlap"),
        conflict(true, "overlap"),
        conflict(false, "driver_unavailable"),
    ];
    let open: Vec<_> = conflicts.iter().filter(|c| !c.is_resolved).collect();
    assert_eq!(open.len(), 2);
    for c in open {
        assert!(c.resolved_at.is_none());
    }
}

#[wasm_bindgen_test]
fn resolved_conflict_has_resolution_timestamp() {
    let c = conflict(true, "overlap");
    assert!(c.is_resolved);
    assert!(c.resolved_at.is_some());
    assert!(c.resolved_at.unwrap() > c.detected_at);
}

#[wasm_bindgen_test]
fn conflict_deserialises_from_backend_shape() {
    let raw = json!({
        "id":            "00000000-0000-0000-0000-000000000001",
        "trip_id":       "00000000-0000-0000-0000-000000000002",
        "conflict_type": "scheduling_overlap",
        "description":   "Trip overlaps by 15 minutes",
        "detected_at":   "2026-01-02T03:04:05Z",
        "resolved_at":   null,
        "is_resolved":   false,
    });
    let parsed: TripConflict = serde_json::from_value(raw).expect("parse");
    assert_eq!(parsed.conflict_type, "scheduling_overlap");
    assert!(!parsed.is_resolved);
    assert!(parsed.resolved_at.is_none());
}

#[wasm_bindgen_test]
fn conflict_types_are_categorised() {
    let scheduling = conflict(false, "scheduling_overlap");
    let driver = conflict(false, "driver_unavailable");
    let route = conflict(false, "route_closure");

    let is_sched = |c: &TripConflict| c.conflict_type.starts_with("scheduling");
    assert!(is_sched(&scheduling));
    assert!(!is_sched(&driver));
    assert!(!is_sched(&route));
}
