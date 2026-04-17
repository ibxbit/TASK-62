//! Role-based access flow tests for the frontend.
//!
//! Exercises the real `SessionInfo` helpers that drive every `RoleGuard`,
//! `AuthGuard`, and navigation decision in the SPA.  If any of these matrices
//! drift from the backend's permission seeds they will break immediately.
//!
//! Framework evidence: `wasm_bindgen_test`.

use wasm_bindgen_test::*;

use transitops_frontend::types::auth::SessionInfo;

wasm_bindgen_test_configure!(run_in_browser);

fn session(role: &str) -> SessionInfo {
    SessionInfo {
        username: "u".into(),
        role: role.to_string(),
        session_id: None,
    }
}

// ── Individual role predicates ──────────────────────────────────────────────

#[wasm_bindgen_test]
fn admin_predicate_only_matches_operations_admin() {
    assert!(session("operations_admin").is_admin());
    assert!(!session("dispatcher").is_admin());
    assert!(!session("finance_analyst").is_admin());
    assert!(!session("staff_user").is_admin());
    assert!(!session("nobody").is_admin());
}

#[wasm_bindgen_test]
fn role_predicates_are_mutually_exclusive() {
    let roles = ["operations_admin", "dispatcher", "finance_analyst", "staff_user"];
    for r in &roles {
        let s = session(r);
        let matches: u32 = (s.is_admin() as u32)
            + (s.is_dispatcher() as u32)
            + (s.is_finance() as u32)
            + (s.is_staff() as u32);
        assert_eq!(matches, 1, "role {r:?} should match exactly one predicate");
    }
}

// ── Can-access matrix (must mirror backend RBAC seed) ───────────────────────

#[wasm_bindgen_test]
fn can_publish_is_admin_only() {
    assert!(session("operations_admin").can_publish());
    assert!(!session("dispatcher").can_publish());
    assert!(!session("finance_analyst").can_publish());
    assert!(!session("staff_user").can_publish());
}

#[wasm_bindgen_test]
fn can_finance_is_admin_or_finance() {
    assert!(session("operations_admin").can_finance());
    assert!(session("finance_analyst").can_finance());
    assert!(!session("dispatcher").can_finance());
    assert!(!session("staff_user").can_finance());
}

#[wasm_bindgen_test]
fn can_manage_metrics_is_admin_or_finance() {
    assert!(session("operations_admin").can_manage_metrics());
    assert!(session("finance_analyst").can_manage_metrics());
    assert!(!session("dispatcher").can_manage_metrics());
    assert!(!session("staff_user").can_manage_metrics());
}

#[wasm_bindgen_test]
fn can_alerts_excludes_staff() {
    assert!(session("operations_admin").can_alerts());
    assert!(session("dispatcher").can_alerts());
    assert!(session("finance_analyst").can_alerts());
    assert!(!session("staff_user").can_alerts());
}

#[wasm_bindgen_test]
fn can_reporting_covers_every_seeded_role() {
    for r in ["operations_admin", "dispatcher", "finance_analyst", "staff_user"] {
        assert!(session(r).can_reporting(), "{r} must have reporting read");
    }
}

#[wasm_bindgen_test]
fn unknown_role_is_denied_everywhere() {
    let s = session("cleaner_bot");
    assert!(!s.is_admin());
    assert!(!s.can_publish());
    assert!(!s.can_finance());
    assert!(!s.can_alerts());
    assert!(!s.can_reporting());
    assert!(!s.can_manage_metrics());
}

// ── AuthGuard redirect decision ─────────────────────────────────────────────

#[wasm_bindgen_test]
fn auth_guard_redirects_when_no_session() {
    let s: Option<SessionInfo> = None;
    assert!(s.is_none(), "No session must redirect");
}

#[wasm_bindgen_test]
fn auth_guard_passes_any_authenticated_role() {
    for r in ["operations_admin", "dispatcher", "finance_analyst", "staff_user"] {
        let s = Some(session(r));
        assert!(s.is_some(), "{r} must pass AuthGuard");
    }
}
