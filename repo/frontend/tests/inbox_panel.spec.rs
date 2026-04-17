//! Inbox / notification panel behaviour tests.
//!
//! Drives the real `NotificationState` reducer (store/notification_store.rs)
//! through the same `Acknowledge`, `Dismiss`, `AcknowledgeAll`,
//! `SetCounts`, `SetFilter`, `ToggleOpen`, `Close`, `SetError` actions the
//! `InboxPanel` component uses.  Verifies that the reducer keeps its
//! unread counter, open/closed state, and filter in the correct invariants.
//!
//! Framework evidence: `wasm_bindgen_test`.

use std::rc::Rc;

use chrono::{TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;
use wasm_bindgen_test::*;
use yew::Reducible;

use transitops_frontend::store::notification_store::{
    NotificationAction, NotificationState,
};
use transitops_frontend::types::notification::{Notification, StatusFilter};

wasm_bindgen_test_configure!(run_in_browser);

fn make_state() -> Rc<NotificationState> {
    Rc::new(NotificationState::default())
}

fn make_notification(id: Uuid, status: &str) -> Notification {
    let ts = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    Notification {
        id,
        event_id: Uuid::nil(),
        event_type: "ops.trip.conflict_detected".to_string(),
        severity: "warning".to_string(),
        source_entity_id: None,
        payload: json!({"title": "Conflict"}),
        status: status.to_string(),
        delivered_at: Some(ts),
        read_at: None,
        created_at: ts,
    }
}

// ── Default state ───────────────────────────────────────────────────────────

#[wasm_bindgen_test]
fn default_state_is_closed_with_zero_counts() {
    let s = NotificationState::default();
    assert!(!s.is_open);
    assert_eq!(s.unread_count, 0);
    assert_eq!(s.queued_count, 0);
    assert!(s.notifications.is_empty());
    assert!(matches!(s.filter, StatusFilter::Unread));
    assert!(!s.loading);
    assert!(s.error.is_none());
}

// ── Open/close lifecycle ────────────────────────────────────────────────────

#[wasm_bindgen_test]
fn toggle_open_flips_is_open() {
    let s = make_state();
    let s = s.reduce(NotificationAction::ToggleOpen);
    assert!(s.is_open);
    let s = s.reduce(NotificationAction::ToggleOpen);
    assert!(!s.is_open);
}

#[wasm_bindgen_test]
fn close_forces_closed_regardless_of_prior_state() {
    let s = make_state();
    let s = s.reduce(NotificationAction::ToggleOpen);
    assert!(s.is_open);
    let s = s.reduce(NotificationAction::Close);
    assert!(!s.is_open);
}

// ── Acknowledge / dismiss semantics ─────────────────────────────────────────

#[wasm_bindgen_test]
fn acknowledge_moves_delivered_to_read_and_decrements_unread() {
    let id = Uuid::new_v4();
    let s = make_state().reduce(NotificationAction::SetNotifications(vec![
        make_notification(id, "delivered"),
    ]));
    let s = s.reduce(NotificationAction::SetCounts { unread: 1, queued: 0 });
    let s = s.reduce(NotificationAction::Acknowledge(id));
    assert_eq!(s.unread_count, 0);
    assert_eq!(s.notifications[0].status, "read");
    assert!(s.notifications[0].read_at.is_some());
}

#[wasm_bindgen_test]
fn acknowledge_already_read_is_noop() {
    let id = Uuid::new_v4();
    let s = make_state().reduce(NotificationAction::SetNotifications(vec![
        make_notification(id, "read"),
    ]));
    let s = s.reduce(NotificationAction::SetCounts { unread: 0, queued: 0 });
    let before = s.notifications[0].status.clone();
    let s = s.reduce(NotificationAction::Acknowledge(id));
    assert_eq!(s.notifications[0].status, before);
    assert_eq!(s.unread_count, 0);
}

#[wasm_bindgen_test]
fn dismiss_removes_item_and_decrements_only_for_unread() {
    let unread_id = Uuid::new_v4();
    let read_id = Uuid::new_v4();
    let s = make_state().reduce(NotificationAction::SetNotifications(vec![
        make_notification(unread_id, "delivered"),
        make_notification(read_id, "read"),
    ]));
    let s = s.reduce(NotificationAction::SetCounts { unread: 1, queued: 0 });

    // Dismissing an already-read item leaves the counter alone.
    let s = s.reduce(NotificationAction::Dismiss(read_id));
    assert_eq!(s.notifications.len(), 1);
    assert_eq!(s.unread_count, 1);

    // Dismissing an unread item drops the counter.
    let s = s.reduce(NotificationAction::Dismiss(unread_id));
    assert!(s.notifications.is_empty());
    assert_eq!(s.unread_count, 0);
}

#[wasm_bindgen_test]
fn acknowledge_all_marks_all_delivered_as_read_and_zeros_counter() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();
    let s = make_state().reduce(NotificationAction::SetNotifications(vec![
        make_notification(a, "delivered"),
        make_notification(b, "delivered"),
        make_notification(c, "queued"),
    ]));
    let s = s.reduce(NotificationAction::SetCounts { unread: 2, queued: 1 });

    let s = s.reduce(NotificationAction::AcknowledgeAll);
    assert_eq!(s.unread_count, 0);
    // Both delivered → read. Queued stays queued (DND semantics).
    assert_eq!(s.notifications[0].status, "read");
    assert_eq!(s.notifications[1].status, "read");
    assert_eq!(s.notifications[2].status, "queued");
}

// ── Filter ──────────────────────────────────────────────────────────────────

#[wasm_bindgen_test]
fn set_filter_changes_active_tab() {
    let s = make_state().reduce(NotificationAction::SetFilter(StatusFilter::Queued));
    assert!(matches!(s.filter, StatusFilter::Queued));
    let s = s.reduce(NotificationAction::SetFilter(StatusFilter::All));
    assert!(matches!(s.filter, StatusFilter::All));
}

// ── Error state ─────────────────────────────────────────────────────────────

#[wasm_bindgen_test]
fn set_error_clears_loading_flag() {
    let s = make_state().reduce(NotificationAction::SetLoading(true));
    assert!(s.loading);
    let s = s.reduce(NotificationAction::SetError(Some("network".into())));
    assert!(!s.loading);
    assert_eq!(s.error.as_deref(), Some("network"));
}

#[wasm_bindgen_test]
fn set_notifications_clears_error_and_loading() {
    let id = Uuid::new_v4();
    let s = make_state()
        .reduce(NotificationAction::SetLoading(true))
        .reduce(NotificationAction::SetError(Some("x".into())))
        .reduce(NotificationAction::SetNotifications(vec![make_notification(id, "delivered")]));
    assert!(!s.loading);
    assert!(s.error.is_none());
    assert_eq!(s.notifications.len(), 1);
}
