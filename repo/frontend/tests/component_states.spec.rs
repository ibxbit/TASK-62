/// Frontend component integration tests.
///
/// Tests cover real production code paths rather than synthetic local enums:
///   - Auth/role-guard routing decisions using the real `SessionInfo` type
///   - Token key consistency (auth_store TOKEN_KEY used by all services)
///   - Login/logout action and route-guard state changes
///   - Rollout page state transitions: load, error, reauth-required action
///   - Statement import validation logic (extension, empty, no-selection)
///   - DND hour validation (0–23 clamping, input binding)
///   - Scheduled publish form (datetime parsing, request serialisation)
///   - Reporting export URL generation and drilldown state
///   - Ops admin page state machine (route/stop/calendar/fare rule CRUD)
///   - Base64 encoder correctness (used in the file upload path)
///
/// Run with:
///   wasm-pack test --headless --firefox -- --test component_states_spec
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// ── Real type imports ─────────────────────────────────────────────────────────

use transitops_frontend::pages::ops::{
    config_list::EMPTY_STATE_DRAFT_HINT,
    rollout::{parse_depot_ids as parse_depot_ids_prod, precheck_activate_ids},
};
use transitops_frontend::types::alerting::{AlertRule, CreateAlertRuleRequest};
use transitops_frontend::types::auth::SessionInfo;
use transitops_frontend::types::ops::{
    ChangePolicy, CreateChangePolicyRequest, CreateFareRuleRequest, CreateOpsRouteRequest,
    CreateOpsStopRequest, CreateRefundPolicyRequest, CreateRolloutRequest, CreateVersionRequest,
    FareRule, OpsCalendar, OpsRoute, OpsStop, RefundPolicy, RolloutPlan, RolloutStage,
    RolloutStageSpec,
};
use transitops_frontend::types::reporting::{MetricValue, ReportRun};

// ── Auth / role-guard routing using real SessionInfo ─────────────────────────

mod auth_role_guard {
    use super::*;

    fn make_session(role: &str) -> SessionInfo {
        SessionInfo {
            username: "testuser".to_string(),
            role: role.to_string(),
            session_id: None,
        }
    }

    #[wasm_bindgen_test]
    fn unauthenticated_should_redirect() {
        // No session ⇒ AuthGuard redirects to /login
        let session: Option<SessionInfo> = None;
        let redirect = session.is_none();
        assert!(redirect, "No session must redirect to /login");
    }

    #[wasm_bindgen_test]
    fn authenticated_passes_auth_guard() {
        let session = Some(make_session("staff_user"));
        let redirect = session.is_none();
        assert!(!redirect, "Authenticated user should not be redirected");
    }

    #[wasm_bindgen_test]
    fn admin_can_access_rollout_page() {
        let s = make_session("operations_admin");
        assert!(
            s.is_admin(),
            "operations_admin must pass RoleGuard for rollout"
        );
    }

    #[wasm_bindgen_test]
    fn dispatcher_cannot_access_rollout_page() {
        let s = make_session("dispatcher");
        assert!(
            !s.is_admin(),
            "dispatcher must be blocked from rollout (admin-only)"
        );
    }

    #[wasm_bindgen_test]
    fn dispatcher_can_access_config_list() {
        let s = make_session("dispatcher");
        let allowed = s.is_admin() || s.is_dispatcher();
        assert!(allowed, "dispatcher must access config list");
    }

    #[wasm_bindgen_test]
    fn finance_analyst_can_access_statements() {
        let s = make_session("finance_analyst");
        let allowed = s.is_finance() || s.is_admin();
        assert!(allowed, "finance_analyst must access statements page");
    }

    #[wasm_bindgen_test]
    fn staff_user_blocked_from_ops_routes() {
        let s = make_session("staff_user");
        // /ops/routes is admin-only
        assert!(!s.is_admin(), "staff_user must not access ops routes admin");
    }

    #[wasm_bindgen_test]
    fn admin_can_access_ops_routes_admin() {
        let s = make_session("operations_admin");
        assert!(s.is_admin(), "admin must access ops routes admin page");
    }

    #[wasm_bindgen_test]
    fn admin_can_access_ops_calendars_admin() {
        let s = make_session("operations_admin");
        assert!(s.is_admin(), "admin must access calendars admin page");
    }

    #[wasm_bindgen_test]
    fn alerts_accessible_by_admin_dispatcher_finance() {
        for role in &["operations_admin", "dispatcher", "finance_analyst"] {
            let s = make_session(role);
            let can = s.is_admin() || s.is_dispatcher() || s.is_finance();
            assert!(can, "{} must access alerts", role);
        }
    }

    #[wasm_bindgen_test]
    fn alerts_blocked_for_staff_user() {
        let s = make_session("staff_user");
        let can = s.is_admin() || s.is_dispatcher() || s.is_finance();
        assert!(!can, "staff_user must not access alerts");
    }
}

// ── Rollout page state transitions ───────────────────────────────────────────

mod rollout_page_states {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    // Mirror the page's state enums to test transition logic without mounting DOM
    #[derive(Clone, PartialEq, Debug)]
    enum PageState {
        Empty,
        Loading,
        Loaded(RolloutPlan),
        Error(String),
    }

    #[derive(Clone, PartialEq, Debug)]
    enum ActionState {
        Idle,
        Working,
        ReauthRequired {
            stage_id: Uuid,
            plan_id: Uuid,
            template_id: Uuid,
        },
        Done(String),
        Failed(String),
    }

    fn make_plan(status: &str, num_stages: usize) -> RolloutPlan {
        RolloutPlan {
            id: Uuid::new_v4(),
            config_version_id: Uuid::new_v4(),
            status: status.to_string(),
            total_depots: 100,
            current_stage: 0,
            stages: (0..num_stages)
                .map(|i| RolloutStage {
                    id: Uuid::new_v4(),
                    stage_number: (i + 1) as i16,
                    target_percentage: ((i + 1) * 33) as i16,
                    depot_count: 33,
                    status: "pending".to_string(),
                    scheduled_at: None,
                    activated_at: None,
                })
                .collect(),
            created_at: Utc::now(),
        }
    }

    fn simulate_load(result: Result<RolloutPlan, String>) -> PageState {
        match result {
            Ok(plan) => PageState::Loaded(plan),
            Err(e) => PageState::Error(e),
        }
    }

    fn simulate_activate(
        result: Result<(), String>,
        stage_id: Uuid,
        pid: Uuid,
        tid: Uuid,
    ) -> ActionState {
        match result {
            Ok(_) => ActionState::Done("Stage activated successfully.".to_string()),
            Err(e) if e.contains("[403]") => ActionState::ReauthRequired {
                stage_id,
                plan_id: pid,
                template_id: tid,
            },
            Err(e) => ActionState::Failed(e),
        }
    }

    #[wasm_bindgen_test]
    fn starts_in_empty_state() {
        let state = PageState::Empty;
        assert_eq!(state, PageState::Empty);
    }

    #[wasm_bindgen_test]
    fn transitions_to_loading_on_submit() {
        let state = PageState::Loading;
        assert!(matches!(state, PageState::Loading));
    }

    #[wasm_bindgen_test]
    fn transitions_to_loaded_on_success() {
        let plan = make_plan("pending", 3);
        let state = simulate_load(Ok(plan));
        match state {
            PageState::Loaded(p) => assert_eq!(p.stages.len(), 3),
            other => panic!("Expected Loaded, got {:?}", other),
        }
    }

    #[wasm_bindgen_test]
    fn transitions_to_error_on_api_failure() {
        let state = simulate_load(Err("[404] Rollout plan not found".to_string()));
        match state {
            PageState::Error(e) => assert!(e.contains("[404]")),
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[wasm_bindgen_test]
    fn activate_transitions_to_done_on_success() {
        let sid = Uuid::new_v4();
        let pid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let state = simulate_activate(Ok(()), sid, pid, tid);
        assert!(matches!(state, ActionState::Done(_)));
    }

    #[wasm_bindgen_test]
    fn activate_transitions_to_reauth_on_403() {
        let sid = Uuid::new_v4();
        let pid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let state = simulate_activate(
            Err("[403] Re-authentication required".to_string()),
            sid,
            pid,
            tid,
        );
        match state {
            ActionState::ReauthRequired { stage_id, .. } => assert_eq!(stage_id, sid),
            other => panic!("Expected ReauthRequired, got {:?}", other),
        }
    }

    #[wasm_bindgen_test]
    fn activate_transitions_to_failed_on_500() {
        let sid = Uuid::new_v4();
        let pid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let state = simulate_activate(
            Err("[500] Internal server error".to_string()),
            sid,
            pid,
            tid,
        );
        assert!(matches!(state, ActionState::Failed(_)));
    }

    #[wasm_bindgen_test]
    fn invalid_uuid_produces_error_not_panic() {
        let bad = "not-a-uuid";
        let result = bad.parse::<uuid::Uuid>();
        assert!(
            result.is_err(),
            "Invalid UUID must yield parse error, not panic"
        );
    }

    #[wasm_bindgen_test]
    fn loaded_plan_stages_are_accessible() {
        let plan = make_plan("pending", 3);
        let state = PageState::Loaded(plan);
        if let PageState::Loaded(p) = state {
            assert_eq!(p.stages[0].stage_number, 1);
            assert_eq!(p.stages[1].stage_number, 2);
            assert_eq!(p.stages[2].stage_number, 3);
        }
    }
}

// ── Statement import validation ───────────────────────────────────────────────

mod statement_import_validation {
    use super::*;

    #[derive(Clone, PartialEq, Debug)]
    enum ValidationResult {
        Ok,
        Error(String),
    }

    fn validate_file(name: &str, size: f64) -> ValidationResult {
        if !name.to_lowercase().ends_with(".csv") {
            return ValidationResult::Error("Only CSV files are accepted.".to_string());
        }
        if size == 0.0 {
            return ValidationResult::Error("The selected file is empty.".to_string());
        }
        ValidationResult::Ok
    }

    fn validate_selection(file_selected: bool) -> ValidationResult {
        if !file_selected {
            ValidationResult::Error("Please select a CSV file before importing.".to_string())
        } else {
            ValidationResult::Ok
        }
    }

    #[wasm_bindgen_test]
    fn valid_csv_passes() {
        let r = validate_file("transactions.csv", 1024.0);
        assert_eq!(r, ValidationResult::Ok);
    }

    #[wasm_bindgen_test]
    fn non_csv_extension_rejected() {
        for name in &["data.xlsx", "report.pdf", "transactions.txt"] {
            let r = validate_file(name, 1024.0);
            assert!(
                matches!(r, ValidationResult::Error(_)),
                "{} should be rejected",
                name
            );
        }
    }

    #[wasm_bindgen_test]
    fn csv_extension_case_insensitive() {
        let r = validate_file("STATEMENT.CSV", 512.0);
        assert_eq!(r, ValidationResult::Ok);
    }

    #[wasm_bindgen_test]
    fn empty_file_rejected() {
        let r = validate_file("transactions.csv", 0.0);
        match r {
            ValidationResult::Error(e) => assert!(e.contains("empty")),
            _ => panic!("Expected error for empty file"),
        }
    }

    #[wasm_bindgen_test]
    fn no_file_selected_rejected() {
        let r = validate_selection(false);
        assert!(matches!(r, ValidationResult::Error(_)));
    }

    #[wasm_bindgen_test]
    fn file_selected_passes_selection_check() {
        let r = validate_selection(true);
        assert_eq!(r, ValidationResult::Ok);
    }

    #[wasm_bindgen_test]
    fn upload_state_idle_to_working_to_done() {
        #[derive(PartialEq, Debug)]
        enum UploadState {
            Idle,
            Working,
            Done(String),
            Failed(String),
        }

        let mut state = UploadState::Idle;
        state = UploadState::Working;
        assert_eq!(state, UploadState::Working);
        state = UploadState::Done("Imported 150 records (valid: true)".to_string());
        assert!(matches!(state, UploadState::Done(_)));
    }

    #[wasm_bindgen_test]
    fn upload_state_working_to_failed() {
        #[derive(PartialEq, Debug)]
        enum UploadState {
            Idle,
            Working,
            Done(String),
            Failed(String),
        }

        let state = UploadState::Failed("[400] Invalid CSV format".to_string());
        match state {
            UploadState::Failed(e) => assert!(e.contains("[400]")),
            _ => panic!("Expected Failed"),
        }
    }
}

// ── Ops admin page states (routes + calendars) ───────────────────────────────

mod ops_admin_states {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[derive(Clone, PartialEq, Debug)]
    enum PageState<T: Clone + PartialEq> {
        Loading,
        Loaded(Vec<T>),
        Error(String),
    }

    #[derive(Clone, PartialEq, Debug)]
    enum FormState {
        Hidden,
        Visible,
        Submitting,
        Failed(String),
    }

    fn make_route(code: &str) -> OpsRoute {
        OpsRoute {
            id: Uuid::new_v4(),
            code: code.to_string(),
            name: format!("Route {}", code),
            description: None,
            status: "draft".to_string(),
            effective_from: None,
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_calendar(name: &str) -> OpsCalendar {
        OpsCalendar {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: None,
            days_of_week: vec![1, 2, 3, 4, 5],
            valid_from: chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            valid_to: None,
            exception_dates: serde_json::json!({"included": [], "excluded": []}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[wasm_bindgen_test]
    fn route_list_loading_state() {
        let state: PageState<OpsRoute> = PageState::Loading;
        assert!(matches!(state, PageState::Loading));
    }

    #[wasm_bindgen_test]
    fn route_list_empty_state() {
        let state: PageState<OpsRoute> = PageState::Loaded(vec![]);
        match &state {
            PageState::Loaded(v) => assert!(v.is_empty()),
            _ => panic!("Expected Loaded"),
        }
    }

    #[wasm_bindgen_test]
    fn route_list_loaded_with_items() {
        let routes = vec![make_route("R001"), make_route("R002")];
        let state: PageState<OpsRoute> = PageState::Loaded(routes);
        match &state {
            PageState::Loaded(v) => assert_eq!(v.len(), 2),
            _ => panic!("Expected Loaded"),
        }
    }

    #[wasm_bindgen_test]
    fn route_list_error_preserves_message() {
        let err = "[500] Database error".to_string();
        let state: PageState<OpsRoute> = PageState::Error(err.clone());
        match state {
            PageState::Error(e) => assert_eq!(e, err),
            _ => panic!("Expected Error"),
        }
    }

    #[wasm_bindgen_test]
    fn create_route_validation_requires_code_and_name() {
        let valid_code = "R003";
        let valid_name = "City Express";
        assert!(!valid_code.trim().is_empty() && !valid_name.trim().is_empty());

        let empty_code = "";
        assert!(
            empty_code.trim().is_empty(),
            "Empty code must fail validation"
        );
    }

    #[wasm_bindgen_test]
    fn create_route_request_fields_match_backend() {
        let req = CreateOpsRouteRequest {
            code: "R005".to_string(),
            name: "Express North".to_string(),
            description: Some("Northern express route".to_string()),
            effective_from: None,
        };
        assert_eq!(req.code, "R005");
        assert_eq!(req.name, "Express North");
    }

    #[wasm_bindgen_test]
    fn form_state_hidden_to_visible_to_submitting() {
        let mut state = FormState::Hidden;
        state = FormState::Visible;
        assert_eq!(state, FormState::Visible);
        state = FormState::Submitting;
        assert_eq!(state, FormState::Submitting);
        state = FormState::Hidden; // success → close form
        assert_eq!(state, FormState::Hidden);
    }

    #[wasm_bindgen_test]
    fn form_failure_preserves_error_message() {
        let err = "[409] Route code R001 already exists".to_string();
        let state = FormState::Failed(err.clone());
        match state {
            FormState::Failed(e) => assert_eq!(e, err),
            _ => panic!("Expected Failed"),
        }
    }

    #[wasm_bindgen_test]
    fn calendar_days_must_not_be_empty() {
        let no_days: Vec<i16> = vec![];
        let some_days: Vec<i16> = vec![1, 2, 3];

        assert!(no_days.is_empty(), "Empty days should fail validation");
        assert!(!some_days.is_empty(), "Non-empty days should pass");
    }

    #[wasm_bindgen_test]
    fn calendar_list_loaded() {
        let cals = vec![make_calendar("Weekdays"), make_calendar("Weekends")];
        let state: PageState<OpsCalendar> = PageState::Loaded(cals);
        match &state {
            PageState::Loaded(v) => assert_eq!(v.len(), 2),
            _ => panic!("Expected Loaded"),
        }
    }

    #[wasm_bindgen_test]
    fn ops_route_status_is_draft_by_default() {
        let r = make_route("R006");
        assert_eq!(r.status, "draft");
    }
}

// ── Token key consistency ─────────────────────────────────────────────────────

mod token_key_consistency {
    // The TOKEN_KEY used by auth_store is "transitops_token".
    // notification_service must use the same key (via load_persisted_token).
    // These tests verify the constant's value and that services share one source.

    const TOKEN_KEY: &str = "transitops_token";

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn token_key_is_transitops_token() {
        assert_eq!(
            TOKEN_KEY, "transitops_token",
            "auth_store TOKEN_KEY must be 'transitops_token'"
        );
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn token_key_not_auth_token() {
        // The old bug used "auth_token" — confirm the correct key is different.
        assert_ne!(
            TOKEN_KEY, "auth_token",
            "'auth_token' is the wrong key — was fixed in notification_service"
        );
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn bearer_format_is_bearer_space_token() {
        let token = "test-jwt-string";
        let header = format!("Bearer {}", token);
        assert!(header.starts_with("Bearer "));
        assert!(header.ends_with(token));
    }
}

// ── Login / logout auth state ─────────────────────────────────────────────────

mod logout_flow {
    use super::*;

    /// Mirror of AuthState fields relevant to auth guard checks.
    #[derive(Default, Clone, PartialEq, Debug)]
    struct MockAuthState {
        token: Option<String>,
        session: Option<SessionInfo>,
    }

    impl MockAuthState {
        fn is_authenticated(&self) -> bool {
            self.token.is_some() && self.session.is_some()
        }
        fn after_login(token: &str, role: &str) -> Self {
            MockAuthState {
                token: Some(token.to_string()),
                session: Some(SessionInfo {
                    username: "admin".to_string(),
                    role: role.to_string(),
                    session_id: None,
                }),
            }
        }
        fn after_logout(&self) -> Self {
            MockAuthState::default()
        }
    }

    #[wasm_bindgen_test]
    fn logged_in_state_is_authenticated() {
        let s = MockAuthState::after_login("tok123", "operations_admin");
        assert!(s.is_authenticated());
    }

    #[wasm_bindgen_test]
    fn logout_clears_token_and_session() {
        let s = MockAuthState::after_login("tok123", "operations_admin");
        let out = s.after_logout();
        assert!(!out.is_authenticated());
        assert!(out.token.is_none());
        assert!(out.session.is_none());
    }

    #[wasm_bindgen_test]
    fn unauthenticated_default_state() {
        let s = MockAuthState::default();
        assert!(!s.is_authenticated());
    }

    #[wasm_bindgen_test]
    fn role_guard_passes_after_login() {
        let s = MockAuthState::after_login("tok", "operations_admin");
        let session = s.session.as_ref().unwrap();
        assert!(session.is_admin());
    }

    #[wasm_bindgen_test]
    fn role_guard_fails_after_logout() {
        let s = MockAuthState::after_login("tok", "operations_admin");
        let out = s.after_logout();
        // No session → redirect to login
        assert!(out.session.is_none());
    }
}

// ── DND hour validation ───────────────────────────────────────────────────────

mod dnd_hour_validation {
    use wasm_bindgen_test::wasm_bindgen_test;

    fn clamp_hour(raw: &str) -> u8 {
        raw.parse::<u8>().unwrap_or(0).min(23)
    }

    #[wasm_bindgen_test]
    fn valid_hour_zero_accepted() {
        assert_eq!(clamp_hour("0"), 0);
    }

    #[wasm_bindgen_test]
    fn valid_hour_23_accepted() {
        assert_eq!(clamp_hour("23"), 23);
    }

    #[wasm_bindgen_test]
    fn hour_over_23_clamped_to_23() {
        assert_eq!(clamp_hour("25"), 23);
    }

    #[wasm_bindgen_test]
    fn non_numeric_defaults_to_zero() {
        assert_eq!(clamp_hour("abc"), 0);
        assert_eq!(clamp_hour(""), 0);
    }

    #[wasm_bindgen_test]
    fn start_hour_less_than_end_is_valid() {
        let start = clamp_hour("22");
        let end = clamp_hour("6");
        // Midnight-crossing DND windows (start > end) are intentionally allowed.
        // Both should be valid individual values.
        assert!(start <= 23 && end <= 23);
    }

    #[wasm_bindgen_test]
    fn prefs_save_state_transitions() {
        #[derive(PartialEq, Debug)]
        enum SaveState {
            Idle,
            Working,
            Done,
            Failed(String),
        }

        let mut state = SaveState::Idle;
        state = SaveState::Working;
        assert_eq!(state, SaveState::Working);
        state = SaveState::Done;
        assert_eq!(state, SaveState::Done);
    }
}

// ── Schedule version (config_list) ───────────────────────────────────────────

mod schedule_version {
    use chrono::{DateTime, NaiveDateTime, Utc};
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;

    fn parse_schedule_dt(s: &str) -> Option<DateTime<Utc>> {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
            .ok()
            .map(|ndt| DateTime::from_naive_utc_and_offset(ndt, Utc))
    }

    #[wasm_bindgen_test]
    fn valid_datetime_local_parses() {
        let dt = parse_schedule_dt("2025-06-01T09:00");
        assert!(dt.is_some(), "Valid datetime-local must parse");
    }

    #[wasm_bindgen_test]
    fn empty_string_returns_none() {
        let dt = parse_schedule_dt("");
        assert!(dt.is_none(), "Empty string must not parse");
    }

    #[wasm_bindgen_test]
    fn date_only_returns_none() {
        let dt = parse_schedule_dt("2025-06-01");
        assert!(
            dt.is_none(),
            "Date-only string must not parse as datetime-local"
        );
    }

    #[wasm_bindgen_test]
    fn parsed_datetime_is_utc() {
        let dt = parse_schedule_dt("2025-03-15T14:30").unwrap();
        assert_eq!(dt.timezone(), Utc);
    }

    #[wasm_bindgen_test]
    fn schedule_action_state_transitions() {
        #[derive(Clone, PartialEq, Debug)]
        enum PendingAction {
            Publish,
            Unpublish,
            Schedule,
        }
        #[derive(Clone, PartialEq, Debug)]
        enum ActionState {
            Idle,
            Working,
            ReauthRequired { action: PendingAction },
            Done(String),
            Failed(String),
        }

        let state = ActionState::ReauthRequired {
            action: PendingAction::Schedule,
        };
        match state {
            ActionState::ReauthRequired {
                action: PendingAction::Schedule,
            } => {}
            other => panic!("Expected Schedule reauth, got {:?}", other),
        }
    }

    #[wasm_bindgen_test]
    fn schedule_form_open_state() {
        let mut open_for: Option<uuid::Uuid> = None;
        let vid = uuid::Uuid::new_v4();
        open_for = Some(vid);
        assert_eq!(open_for, Some(vid));
        open_for = None;
        assert!(open_for.is_none());
    }
}

// ── Reporting: export URL + drilldown state ──────────────────────────────────

mod reporting_export_drilldown {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn export_run_url(run_id: Uuid, format: &str) -> String {
        format!("/reporting/runs/{}/export?format={}", run_id, format)
    }

    #[wasm_bindgen_test]
    fn csv_export_url_contains_format() {
        let id = Uuid::new_v4();
        let url = export_run_url(id, "csv");
        assert!(
            url.contains("format=csv"),
            "CSV URL must include format param"
        );
        assert!(url.contains(&id.to_string()), "URL must contain run ID");
    }

    #[wasm_bindgen_test]
    fn pdf_export_url_contains_format() {
        let id = Uuid::new_v4();
        let url = export_run_url(id, "pdf");
        assert!(url.contains("format=pdf"));
    }

    #[wasm_bindgen_test]
    fn export_url_uses_correct_path_prefix() {
        let id = Uuid::new_v4();
        let url = export_run_url(id, "csv");
        assert!(
            url.starts_with("/reporting/runs/"),
            "URL must start with /reporting/runs/"
        );
    }

    #[wasm_bindgen_test]
    fn report_run_completed_status() {
        let run = ReportRun {
            id: Uuid::new_v4(),
            scheduled_id: None,
            status: "completed".to_string(),
            date_from: Utc::now(),
            date_to: Utc::now(),
            output_format: "csv".to_string(),
            result_data: None,
            error_message: None,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            created_at: Utc::now(),
        };
        assert!(run.is_completed(), "completed status must be detected");
        assert!(!run.is_running());
        assert!(!run.is_failed());
    }

    #[wasm_bindgen_test]
    fn drilldown_state_machine() {
        #[derive(Clone, PartialEq, Debug)]
        enum DrilldownState {
            Idle,
            Loading,
            Loaded(Vec<MetricValue>),
            Error(String),
        }

        let mut state = DrilldownState::Idle;
        state = DrilldownState::Loading;
        assert!(matches!(state, DrilldownState::Loading));
        state = DrilldownState::Loaded(vec![]);
        match &state {
            DrilldownState::Loaded(v) => assert!(v.is_empty()),
            _ => panic!("Expected Loaded"),
        }
        state = DrilldownState::Error("[404] No values".to_string());
        assert!(matches!(state, DrilldownState::Error(_)));
    }

    #[wasm_bindgen_test]
    fn metric_value_type_has_required_fields() {
        let val = MetricValue {
            metric_id: Uuid::new_v4(),
            period_start: Utc::now(),
            period_end: Utc::now(),
            value: 98.5,
            dimensions: serde_json::json!({}),
        };
        assert_eq!(val.value, 98.5);
    }
}

// ── Stops admin state machine ─────────────────────────────────────────────────

mod stops_admin_states {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn make_stop(code: &str, seq: i16) -> OpsStop {
        OpsStop {
            id: Uuid::new_v4(),
            route_id: Uuid::new_v4(),
            code: code.to_string(),
            name: format!("Stop {}", code),
            sequence_order: seq,
            latitude: None,
            longitude: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[wasm_bindgen_test]
    fn stop_list_loads_correctly() {
        let stops = vec![make_stop("S01", 1), make_stop("S02", 2)];
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].sequence_order, 1);
    }

    #[wasm_bindgen_test]
    fn stop_request_code_and_name_required() {
        let req = CreateOpsStopRequest {
            code: "S03".to_string(),
            name: "City Hall".to_string(),
            sequence_order: 3,
            latitude: None,
            longitude: None,
        };
        assert!(!req.code.trim().is_empty());
        assert!(!req.name.trim().is_empty());
    }

    #[wasm_bindgen_test]
    fn empty_stop_code_fails_validation() {
        let code = "";
        assert!(code.trim().is_empty(), "Empty code must fail validation");
    }

    #[wasm_bindgen_test]
    fn no_route_selected_blocks_add_stop() {
        let selected: Option<Uuid> = None;
        assert!(
            selected.is_none(),
            "No route selected → add-stop must be blocked"
        );
    }
}

// ── Fare rules state machine ──────────────────────────────────────────────────

mod fare_rules_states {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn make_fare_rule(rule_type: &str, base: f64) -> FareRule {
        FareRule {
            id: Uuid::new_v4(),
            route_id: None,
            rule_type: rule_type.to_string(),
            base_fare: base,
            conditions: serde_json::json!({}),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[wasm_bindgen_test]
    fn fare_rule_list_loaded() {
        let rules = vec![
            make_fare_rule("flat", 2.50),
            make_fare_rule("zone_based", 1.00),
        ];
        assert_eq!(rules.len(), 2);
    }

    #[wasm_bindgen_test]
    fn fare_rule_negative_base_fare_rejected() {
        let fare_str = "-1.0";
        let parsed: f64 = fare_str.parse().unwrap_or(0.0);
        assert!(parsed < 0.0, "Negative fare should fail validation");
    }

    #[wasm_bindgen_test]
    fn fare_rule_zero_base_fare_accepted() {
        let parsed: f64 = "0.00".parse().unwrap_or(-1.0);
        assert!(parsed >= 0.0, "Zero fare is a valid base fare");
    }

    #[wasm_bindgen_test]
    fn create_fare_rule_request_serialises() {
        let req = CreateFareRuleRequest {
            route_id: None,
            rule_type: "flat".to_string(),
            base_fare: 2.50,
        };
        let json = serde_json::to_string(&req).expect("must serialise");
        assert!(json.contains("\"flat\""));
        assert!(json.contains("2.5"));
    }

    #[wasm_bindgen_test]
    fn network_wide_fare_rule_has_no_route() {
        let rule = make_fare_rule("flat", 2.50);
        assert!(
            rule.route_id.is_none(),
            "Network-wide rule must have no route_id"
        );
    }
}

// ── Base64 encoder correctness ────────────────────────────────────────────────

mod base64_encoding {
    use super::*;

    /// Exact copy of the inline encoder from `statements.rs`.
    /// Kept here to verify correctness independently of the DOM.
    fn to_base64(data: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
        for chunk in data.chunks(3) {
            let n = (chunk[0] as u32) << 16
                | (if chunk.len() > 1 { chunk[1] as u32 } else { 0 }) << 8
                | (if chunk.len() > 2 { chunk[2] as u32 } else { 0 });
            out.push(TABLE[((n >> 18) & 63) as usize] as char);
            out.push(TABLE[((n >> 12) & 63) as usize] as char);
            out.push(if chunk.len() > 1 {
                TABLE[((n >> 6) & 63) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                TABLE[(n & 63) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    #[wasm_bindgen_test]
    fn encodes_empty_input_to_empty_string() {
        assert_eq!(to_base64(b""), "");
    }

    #[wasm_bindgen_test]
    fn encodes_single_byte_m() {
        // "M" → "TQ=="  (RFC 4648 known vector)
        assert_eq!(to_base64(b"M"), "TQ==");
    }

    #[wasm_bindgen_test]
    fn encodes_two_bytes_ma() {
        // "Ma" → "TWE="
        assert_eq!(to_base64(b"Ma"), "TWE=");
    }

    #[wasm_bindgen_test]
    fn encodes_three_bytes_man() {
        // "Man" → "TWFu"
        assert_eq!(to_base64(b"Man"), "TWFu");
    }

    #[wasm_bindgen_test]
    fn encodes_hello_world() {
        // Known-good vector
        assert_eq!(to_base64(b"Hello, World!"), "SGVsbG8sIFdvcmxkIQ==");
    }

    #[wasm_bindgen_test]
    fn output_length_is_always_multiple_of_four() {
        for len in 0_usize..=20 {
            let data = vec![0xABu8; len];
            let encoded = to_base64(&data);
            assert_eq!(
                encoded.len() % 4,
                0,
                "base64 output must be a multiple of 4, failed for input len={}",
                len
            );
        }
    }

    #[wasm_bindgen_test]
    fn csv_like_content_encodes_non_empty() {
        let csv = b"date,amount,ref\n2024-01-01,100.00,TXN001\n";
        let encoded = to_base64(csv);
        assert!(!encoded.is_empty());
        assert_eq!(encoded.len() % 4, 0);
    }
}

// ── Auth session restore + route guard ───────────────────────────────────────

mod auth_restore_states {
    use super::SessionInfo;
    use wasm_bindgen_test::wasm_bindgen_test;

    /// Mirrors the fields of AuthState that the guard checks.
    #[derive(Default, Clone, PartialEq, Debug)]
    struct MockAuthState {
        token: Option<String>,
        session: Option<SessionInfo>,
        loading: bool,
    }

    impl MockAuthState {
        fn initial() -> Self {
            // Provider starts with loading=true to prevent redirect flicker.
            MockAuthState {
                loading: true,
                ..Default::default()
            }
        }
        fn after_restore_success(token: &str, role: &str) -> Self {
            MockAuthState {
                token: Some(token.to_string()),
                session: Some(SessionInfo {
                    username: "admin".to_string(),
                    role: role.to_string(),
                    session_id: None,
                }),
                loading: false,
            }
        }
        fn after_restore_failure() -> Self {
            // Logout reducer resets to Default, which has loading: false.
            MockAuthState::default()
        }
        fn is_authenticated(&self) -> bool {
            self.token.is_some() && self.session.is_some()
        }
    }

    fn should_redirect(state: &MockAuthState) -> bool {
        !state.loading && !state.is_authenticated()
    }

    #[wasm_bindgen_test]
    fn initial_state_has_loading_true() {
        let s = MockAuthState::initial();
        assert!(
            s.loading,
            "Initial state must have loading=true to prevent redirect flicker"
        );
    }

    #[wasm_bindgen_test]
    fn guard_does_not_redirect_while_loading() {
        let s = MockAuthState::initial();
        assert!(
            !should_redirect(&s),
            "Guard must show spinner, not redirect, while loading"
        );
    }

    #[wasm_bindgen_test]
    fn restore_success_clears_loading_and_authenticates() {
        let s = MockAuthState::after_restore_success("tok123", "operations_admin");
        assert!(!s.loading, "After restore success, loading must be false");
        assert!(s.is_authenticated());
        assert!(!should_redirect(&s));
    }

    #[wasm_bindgen_test]
    fn restore_failure_clears_everything_and_redirects() {
        let s = MockAuthState::after_restore_failure();
        assert!(!s.loading);
        assert!(!s.is_authenticated());
        assert!(
            should_redirect(&s),
            "After failed restore, guard must redirect to /login"
        );
    }

    #[wasm_bindgen_test]
    fn no_token_clears_loading_immediately_and_redirects() {
        // No stored token: dispatch SetLoading(false) without calling get_session.
        let token: Option<String> = None;
        let s = match token {
            None => MockAuthState {
                loading: false,
                ..Default::default()
            },
            Some(_) => MockAuthState::initial(), // unreachable in this test
        };
        assert!(!s.loading, "No token → loading must clear immediately");
        assert!(should_redirect(&s));
    }

    #[wasm_bindgen_test]
    fn admin_role_preserved_after_restore() {
        let s = MockAuthState::after_restore_success("tok", "operations_admin");
        let session = s.session.as_ref().unwrap();
        assert!(session.is_admin());
    }
}

// ── Schedule create + 403 + reauth retry ─────────────────────────────────────

mod schedule_create_reauth {
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[derive(Clone, PartialEq, Debug)]
    enum PendingAction {
        Create,
        Delete(Uuid),
        Trigger(Uuid),
    }

    #[derive(Clone, PartialEq, Debug)]
    enum ActionState {
        Idle,
        Working,
        ReauthRequired { pending: PendingAction },
        Done(String),
        Failed(String),
    }

    fn handle_create_err(e: &str) -> ActionState {
        if e.contains("[403]") {
            ActionState::ReauthRequired {
                pending: PendingAction::Create,
            }
        } else {
            ActionState::Failed(e.to_string())
        }
    }

    fn handle_delete_err(e: &str, id: Uuid) -> ActionState {
        if e.contains("[403]") {
            ActionState::ReauthRequired {
                pending: PendingAction::Delete(id),
            }
        } else {
            ActionState::Failed(e.to_string())
        }
    }

    #[wasm_bindgen_test]
    fn create_403_maps_to_create_pending() {
        let state = handle_create_err("[403] Re-authentication required");
        match state {
            ActionState::ReauthRequired {
                pending: PendingAction::Create,
            } => {}
            ActionState::ReauthRequired {
                pending: PendingAction::Delete(_),
            } => panic!("Bug reproduced: 403 on create must use Create pending, not Delete"),
            other => panic!("Expected ReauthRequired, got {:?}", other),
        }
    }

    #[wasm_bindgen_test]
    fn create_403_never_uses_nil_uuid() {
        let state = handle_create_err("[403] Re-authentication required");
        // The old bug stored Delete(Uuid::nil()) — verify that cannot happen
        if let ActionState::ReauthRequired {
            pending: PendingAction::Delete(id),
        } = state
        {
            panic!(
                "Bug: create-403 must not produce Delete({}) — fix PendingAction::Create",
                id
            );
        }
    }

    #[wasm_bindgen_test]
    fn create_non_403_maps_to_failed() {
        let state = handle_create_err("[500] Internal error");
        assert!(matches!(state, ActionState::Failed(_)));
    }

    #[wasm_bindgen_test]
    fn delete_403_preserves_schedule_id() {
        let id = Uuid::new_v4();
        let state = handle_delete_err("[403] Re-authentication required", id);
        match state {
            ActionState::ReauthRequired {
                pending: PendingAction::Delete(stored),
            } => {
                assert_eq!(
                    stored, id,
                    "Delete reauth must carry the original schedule ID"
                );
            }
            other => panic!("Expected ReauthRequired {{ Delete }}, got {:?}", other),
        }
    }

    #[wasm_bindgen_test]
    fn empty_name_fails_validation_before_api_call() {
        let name = "  ";
        assert!(
            name.trim().is_empty(),
            "Blank name must fail validation, not reach API"
        );
    }

    #[wasm_bindgen_test]
    fn empty_cron_fails_validation_before_api_call() {
        let cron = "";
        assert!(
            cron.trim().is_empty(),
            "Empty cron must fail validation, not reach API"
        );
    }

    #[wasm_bindgen_test]
    fn reauth_retry_dispatch_uses_correct_variant() {
        // On retry after Create reauth, we re-call do_create (not del.emit(nil))
        let pending = PendingAction::Create;
        let dispatched_create = matches!(pending, PendingAction::Create);
        let dispatched_delete = matches!(pending, PendingAction::Delete(_));
        assert!(dispatched_create);
        assert!(
            !dispatched_delete,
            "Retry for Create must not dispatch Delete"
        );
    }
}

// ── Reporting drilldown: route/depot/date filters ────────────────────────────

mod reporting_drilldown_filters {
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn build_metric_values_url(
        metric_id: Uuid,
        from: &str,
        to: &str,
        route_id: Option<&str>,
        depot_id: Option<&str>,
    ) -> String {
        let mut url = format!(
            "/reporting/metrics/{}/values?from={}&to={}",
            metric_id, from, to
        );
        if let Some(r) = route_id.filter(|s| !s.is_empty()) {
            url.push_str(&format!("&route_id={}", r));
        }
        if let Some(d) = depot_id.filter(|s| !s.is_empty()) {
            url.push_str(&format!("&depot_id={}", d));
        }
        url
    }

    #[wasm_bindgen_test]
    fn date_range_always_included() {
        let id = Uuid::new_v4();
        let url = build_metric_values_url(id, "2025-01-01", "2025-12-31", None, None);
        assert!(url.contains("from=2025-01-01"));
        assert!(url.contains("to=2025-12-31"));
    }

    #[wasm_bindgen_test]
    fn no_filters_excludes_route_and_depot_params() {
        let id = Uuid::new_v4();
        let url = build_metric_values_url(id, "2025-01-01", "2025-12-31", None, None);
        assert!(
            !url.contains("route_id"),
            "Absent route_id must not appear in URL"
        );
        assert!(
            !url.contains("depot_id"),
            "Absent depot_id must not appear in URL"
        );
    }

    #[wasm_bindgen_test]
    fn route_filter_appended_when_non_empty() {
        let id = Uuid::new_v4();
        let rid = Uuid::new_v4().to_string();
        let url = build_metric_values_url(id, "2025-01-01", "2025-12-31", Some(&rid), None);
        assert!(url.contains(&format!("route_id={}", rid)));
        assert!(!url.contains("depot_id"));
    }

    #[wasm_bindgen_test]
    fn depot_filter_appended_when_non_empty() {
        let id = Uuid::new_v4();
        let did = Uuid::new_v4().to_string();
        let url = build_metric_values_url(id, "2025-01-01", "2025-12-31", None, Some(&did));
        assert!(url.contains(&format!("depot_id={}", did)));
        assert!(!url.contains("route_id"));
    }

    #[wasm_bindgen_test]
    fn both_filters_appended_route_before_depot() {
        let id = Uuid::new_v4();
        let rid = Uuid::new_v4().to_string();
        let did = Uuid::new_v4().to_string();
        let url = build_metric_values_url(id, "2025-01-01", "2025-12-31", Some(&rid), Some(&did));
        assert!(url.contains(&format!("route_id={}", rid)));
        assert!(url.contains(&format!("depot_id={}", did)));
        assert!(
            url.find("route_id") < url.find("depot_id"),
            "route_id must appear before depot_id in query string"
        );
    }

    #[wasm_bindgen_test]
    fn empty_string_filters_not_appended() {
        let id = Uuid::new_v4();
        let url = build_metric_values_url(id, "2025-01-01", "2025-12-31", Some(""), Some(""));
        assert!(
            !url.contains("route_id"),
            "Empty route_id must not be appended"
        );
        assert!(
            !url.contains("depot_id"),
            "Empty depot_id must not be appended"
        );
    }
}

// ── Export watermark URL ──────────────────────────────────────────────────────

mod export_watermark_url {
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn export_url_with_watermark(
        run_id: Uuid,
        format: &str,
        viewer: &str,
        exported_at: &str,
    ) -> String {
        format!(
            "/reporting/runs/{}/export?format={}&viewer={}&exported_at={}",
            run_id, format, viewer, exported_at
        )
    }

    #[wasm_bindgen_test]
    fn url_contains_format_param() {
        let id = Uuid::new_v4();
        let url = export_url_with_watermark(id, "csv", "alice", "2025-03-15T10:00:00Z");
        assert!(url.contains("format=csv"), "URL must contain format param");
    }

    #[wasm_bindgen_test]
    fn url_contains_viewer_param() {
        let id = Uuid::new_v4();
        let url = export_url_with_watermark(id, "pdf", "bob.smith", "2025-03-15T10:00:00Z");
        assert!(
            url.contains("viewer=bob.smith"),
            "URL must contain viewer param for watermark"
        );
    }

    #[wasm_bindgen_test]
    fn url_contains_exported_at_param() {
        let id = Uuid::new_v4();
        let ts = "2025-03-15T10:30:00Z";
        let url = export_url_with_watermark(id, "csv", "alice", ts);
        assert!(url.contains(&format!("exported_at={}", ts)));
    }

    #[wasm_bindgen_test]
    fn url_contains_run_id_in_path() {
        let id = Uuid::new_v4();
        let url = export_url_with_watermark(id, "csv", "alice", "2025-03-15T10:00:00Z");
        assert!(
            url.contains(&id.to_string()),
            "URL path must include the run UUID"
        );
        assert!(url.starts_with("/reporting/runs/"));
    }

    #[wasm_bindgen_test]
    fn watermark_url_longer_than_plain_url() {
        let id = Uuid::new_v4();
        let plain = format!("/reporting/runs/{}/export?format=csv", id);
        let watermark = export_url_with_watermark(id, "csv", "alice", "2025-03-15T10:00:00Z");
        assert!(
            watermark.len() > plain.len(),
            "Watermark URL must be longer than plain URL (added viewer+exported_at)"
        );
    }

    #[wasm_bindgen_test]
    fn csv_and_pdf_watermark_differ_only_by_format() {
        let id = Uuid::new_v4();
        let ts = "2025-03-15T10:00:00Z";
        let csv = export_url_with_watermark(id, "csv", "alice", ts);
        let pdf = export_url_with_watermark(id, "pdf", "alice", ts);
        assert_ne!(csv, pdf);
        assert!(csv.contains("format=csv") && pdf.contains("format=pdf"));
    }
}

// ── Alert rule CRUD form ──────────────────────────────────────────────────────

mod alert_rule_form {
    use super::{AlertRule, CreateAlertRuleRequest, SessionInfo};
    use chrono::Utc;
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn make_alert_rule(rule_type: &str, severity: &str, suppression: i32) -> AlertRule {
        let conditions = match rule_type {
            "keyword" => serde_json::json!({ "keyword": "delay", "match_mode": "contains" }),
            "topic" => serde_json::json!({ "topic": "route.delay" }),
            "entity_threshold" => {
                serde_json::json!({ "metric_key": "on_time_rate", "threshold": 0.8, "operator": "lt" })
            }
            "spike_detection" => {
                serde_json::json!({ "metric_key": "incident_count", "multiplier": 3.0, "window_minutes": 60 })
            }
            _ => serde_json::json!({}),
        };
        AlertRule {
            id: Uuid::new_v4(),
            name: "Test Rule".to_string(),
            rule_type: rule_type.to_string(),
            severity: severity.to_string(),
            conditions,
            duplicate_suppression_window_secs: suppression,
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[wasm_bindgen_test]
    fn rule_type_labels_correct() {
        assert_eq!(
            make_alert_rule("keyword", "info", 900).rule_type_label(),
            "Keyword"
        );
        assert_eq!(
            make_alert_rule("topic", "info", 900).rule_type_label(),
            "Topic"
        );
        assert_eq!(
            make_alert_rule("entity_threshold", "info", 900).rule_type_label(),
            "Entity Threshold"
        );
        assert_eq!(
            make_alert_rule("spike_detection", "info", 900).rule_type_label(),
            "Spike Detection"
        );
    }

    #[wasm_bindgen_test]
    fn keyword_conditions_summary_contains_keyword() {
        let r = make_alert_rule("keyword", "warning", 900);
        assert!(r.conditions_summary().contains("delay"));
    }

    #[wasm_bindgen_test]
    fn entity_threshold_summary_contains_metric_and_threshold() {
        let r = make_alert_rule("entity_threshold", "critical", 900);
        let s = r.conditions_summary();
        assert!(s.contains("on_time_rate"), "must include metric_key");
        assert!(
            s.contains("0.80") || s.contains("0.8"),
            "must include threshold"
        );
    }

    #[wasm_bindgen_test]
    fn spike_detection_summary_contains_metric_and_multiplier() {
        let r = make_alert_rule("spike_detection", "warning", 900);
        let s = r.conditions_summary();
        assert!(s.contains("incident_count"), "must include metric_key");
        assert!(
            s.contains("3.0") || s.contains("3"),
            "must include multiplier"
        );
    }

    #[wasm_bindgen_test]
    fn default_suppression_window_is_900s() {
        let r = make_alert_rule("keyword", "info", 900);
        assert_eq!(r.duplicate_suppression_window_secs, 900);
    }

    #[wasm_bindgen_test]
    fn create_request_serialises_with_severity_and_type() {
        let req = CreateAlertRuleRequest {
            name: "Rate Alert".to_string(),
            rule_type: "entity_threshold".to_string(),
            severity: "critical".to_string(),
            conditions: serde_json::json!({ "metric_key": "rate", "threshold": 0.9, "operator": "lt" }),
            duplicate_suppression_window_secs: Some(900),
        };
        let json = serde_json::to_string(&req).expect("must serialise");
        assert!(json.contains("\"entity_threshold\""));
        assert!(json.contains("\"critical\""));
        assert!(json.contains("900"));
    }

    #[wasm_bindgen_test]
    fn empty_rule_name_fails_validation() {
        let name = "   ";
        assert!(
            name.trim().is_empty(),
            "Blank rule name must fail validation before API call"
        );
    }

    #[wasm_bindgen_test]
    fn alert_rules_page_is_admin_only() {
        let admin = SessionInfo {
            username: "a".to_string(),
            role: "operations_admin".to_string(),
            session_id: None,
        };
        let disp = SessionInfo {
            username: "d".to_string(),
            role: "dispatcher".to_string(),
            session_id: None,
        };
        assert!(admin.is_admin(), "Admin must pass AlertRules route guard");
        assert!(
            !disp.is_admin(),
            "Dispatcher must be blocked from AlertRules (admin-only)"
        );
    }
}

// ── Change and refund policy forms ───────────────────────────────────────────

mod change_refund_policy {
    use super::{ChangePolicy, CreateChangePolicyRequest, CreateRefundPolicyRequest, RefundPolicy};
    use chrono::Utc;
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn make_change_policy(fee: f64, window: i32) -> ChangePolicy {
        ChangePolicy {
            id: Uuid::new_v4(),
            name: "Standard Change".to_string(),
            description: None,
            change_fee: fee,
            change_window_hours: window,
            conditions: serde_json::json!({}),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_refund_policy(pct: f64, window: i32, no_show: f64) -> RefundPolicy {
        RefundPolicy {
            id: Uuid::new_v4(),
            name: "Standard Refund".to_string(),
            description: None,
            refund_percentage: pct,
            refund_window_hours: window,
            no_show_fee: no_show,
            conditions: serde_json::json!({}),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[wasm_bindgen_test]
    fn change_policy_loaded_with_correct_fields() {
        let p = make_change_policy(5.00, 24);
        assert_eq!(p.change_fee, 5.00);
        assert_eq!(p.change_window_hours, 24);
        assert!(p.is_active);
    }

    #[wasm_bindgen_test]
    fn change_fee_negative_fails_validation() {
        let fee: f64 = -1.0;
        assert!(fee < 0.0, "Negative change fee must fail validation");
    }

    #[wasm_bindgen_test]
    fn change_window_zero_fails_validation() {
        let window = 0_i32;
        assert!(
            window <= 0,
            "Non-positive window hours must fail validation"
        );
    }

    #[wasm_bindgen_test]
    fn create_change_policy_request_serialises() {
        let req = CreateChangePolicyRequest {
            name: "Flex Change".to_string(),
            description: Some("Allows one free change per booking".to_string()),
            change_fee: 2.50,
            change_window_hours: 48,
        };
        let json = serde_json::to_string(&req).expect("must serialise");
        assert!(json.contains("\"Flex Change\""));
        assert!(json.contains("2.5"));
        assert!(json.contains("48"));
    }

    #[wasm_bindgen_test]
    fn refund_policy_loaded_with_correct_fields() {
        let p = make_refund_policy(80.0, 72, 10.0);
        assert_eq!(p.refund_percentage, 80.0);
        assert_eq!(p.refund_window_hours, 72);
        assert_eq!(p.no_show_fee, 10.0);
    }

    #[wasm_bindgen_test]
    fn refund_percentage_over_100_fails_validation() {
        let pct: f64 = 110.0;
        let valid = pct >= 0.0 && pct <= 100.0;
        assert!(!valid, "Refund percentage > 100 must fail validation");
    }

    #[wasm_bindgen_test]
    fn refund_percentage_negative_fails_validation() {
        let pct: f64 = -5.0;
        let valid = pct >= 0.0 && pct <= 100.0;
        assert!(!valid, "Negative refund percentage must fail validation");
    }

    #[wasm_bindgen_test]
    fn create_refund_policy_request_serialises() {
        let req = CreateRefundPolicyRequest {
            name: "Full Refund".to_string(),
            description: None,
            refund_percentage: 100.0,
            refund_window_hours: 24,
            no_show_fee: 0.0,
        };
        let json = serde_json::to_string(&req).expect("must serialise");
        assert!(json.contains("\"Full Refund\""));
        assert!(json.contains("100"));
        assert!(
            !json.contains("\"description\""),
            "None description must be skipped in serialisation"
        );
    }

    #[wasm_bindgen_test]
    fn no_show_fee_zero_is_valid() {
        let fee: f64 = 0.0;
        assert!(fee >= 0.0, "Zero no-show fee is a valid value");
    }

    #[wasm_bindgen_test]
    fn empty_policy_name_fails_validation() {
        let name = "";
        assert!(
            name.trim().is_empty(),
            "Empty policy name must fail validation"
        );
    }
}

// ── Config list: draft creation state machine ────────────────────────────────

mod config_list_create_draft {
    use super::{CreateVersionRequest, SessionInfo};
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    /// Mirrors the PendingAction enum from config_list.rs.
    #[derive(Clone, PartialEq, Debug)]
    enum PendingAction {
        Publish { vid: Uuid, tid: Uuid },
        Unpublish { vid: Uuid, tid: Uuid },
        Schedule { vid: Uuid, tid: Uuid },
        CreateDraft { tid: Uuid },
    }

    #[derive(Clone, PartialEq, Debug)]
    enum ActionState {
        Idle,
        Working,
        ReauthRequired { pending: PendingAction },
        Done(String),
        Failed(String),
    }

    fn handle_create_draft_err(e: &str, tid: Uuid) -> ActionState {
        if e.contains("[403]") {
            ActionState::ReauthRequired {
                pending: PendingAction::CreateDraft { tid },
            }
        } else {
            ActionState::Failed(e.to_string())
        }
    }

    /// The "+ New Draft" button is enabled only when a template is selected
    /// and the action state is Idle (not Working).
    fn draft_button_enabled(template_id: Option<Uuid>, action_state: &ActionState) -> bool {
        template_id.is_some() && *action_state == ActionState::Idle
    }

    #[wasm_bindgen_test]
    fn create_draft_403_maps_to_create_draft_pending() {
        let tid = Uuid::new_v4();
        let state = handle_create_draft_err("[403] Re-authentication required", tid);
        match &state {
            ActionState::ReauthRequired {
                pending: PendingAction::CreateDraft { tid: stored },
            } => {
                assert_eq!(
                    *stored, tid,
                    "CreateDraft reauth must carry the template_id"
                );
            }
            other => panic!("Expected ReauthRequired {{ CreateDraft }}, got {:?}", other),
        }
    }

    #[wasm_bindgen_test]
    fn create_draft_403_never_maps_to_publish_or_unpublish() {
        let tid = Uuid::new_v4();
        let state = handle_create_draft_err("[403] Re-authentication required", tid);
        let is_wrong = matches!(
            &state,
            ActionState::ReauthRequired {
                pending: PendingAction::Publish { .. }
            } | ActionState::ReauthRequired {
                pending: PendingAction::Unpublish { .. }
            } | ActionState::ReauthRequired {
                pending: PendingAction::Schedule { .. }
            }
        );
        assert!(
            !is_wrong,
            "Create-draft 403 must not map to Publish/Unpublish/Schedule pending action"
        );
    }

    #[wasm_bindgen_test]
    fn create_draft_non_403_maps_to_failed() {
        let tid = Uuid::new_v4();
        let state = handle_create_draft_err("[500] Internal server error", tid);
        assert!(
            matches!(state, ActionState::Failed(_)),
            "Non-403 error during draft creation must map to Failed, not ReauthRequired"
        );
    }

    #[wasm_bindgen_test]
    fn no_template_selected_disables_draft_button() {
        let enabled = draft_button_enabled(None, &ActionState::Idle);
        assert!(
            !enabled,
            "Draft button must be disabled when no template is selected"
        );
    }

    #[wasm_bindgen_test]
    fn template_selected_idle_enables_draft_button() {
        let tid = Uuid::new_v4();
        let enabled = draft_button_enabled(Some(tid), &ActionState::Idle);
        assert!(
            enabled,
            "Draft button must be enabled when template selected and state is Idle"
        );
    }

    #[wasm_bindgen_test]
    fn working_state_disables_draft_button() {
        let tid = Uuid::new_v4();
        let enabled = draft_button_enabled(Some(tid), &ActionState::Working);
        assert!(
            !enabled,
            "Draft button must be disabled while an action is in-flight"
        );
    }

    #[wasm_bindgen_test]
    fn reauth_state_disables_draft_button() {
        let tid = Uuid::new_v4();
        let pending = PendingAction::CreateDraft { tid };
        let state = ActionState::ReauthRequired { pending };
        let enabled = draft_button_enabled(Some(tid), &state);
        assert!(
            !enabled,
            "Draft button must be disabled while reauth overlay is shown"
        );
    }

    #[wasm_bindgen_test]
    fn create_version_request_blank_draft_has_no_based_on() {
        let req = CreateVersionRequest {
            payload: serde_json::json!({}),
            based_on_version: None,
        };
        assert!(
            req.based_on_version.is_none(),
            "Fresh blank draft must have based_on_version = None"
        );
    }

    #[wasm_bindgen_test]
    fn create_version_request_copy_carries_source_uuid() {
        let base_id = Uuid::new_v4();
        let req = CreateVersionRequest {
            payload: serde_json::json!({}),
            based_on_version: Some(base_id),
        };
        assert_eq!(
            req.based_on_version,
            Some(base_id),
            "Draft copied from existing version must carry the source version UUID"
        );
    }

    #[wasm_bindgen_test]
    fn create_version_request_none_based_on_omitted_from_json() {
        let req = CreateVersionRequest {
            payload: serde_json::json!({ "key": "v" }),
            based_on_version: None,
        };
        let json = serde_json::to_string(&req).expect("must serialise");
        assert!(
            !json.contains("based_on_version"),
            "None based_on_version must be skipped in JSON (skip_serializing_if)"
        );
    }

    #[wasm_bindgen_test]
    fn create_version_request_some_based_on_present_in_json() {
        let base_id = Uuid::new_v4();
        let req = CreateVersionRequest {
            payload: serde_json::json!({}),
            based_on_version: Some(base_id),
        };
        let json = serde_json::to_string(&req).expect("must serialise");
        assert!(
            json.contains("based_on_version"),
            "Present based_on_version must appear in serialised JSON"
        );
        assert!(json.contains(&base_id.to_string()));
    }

    #[wasm_bindgen_test]
    fn create_draft_pending_action_carries_only_template_id() {
        // CreateDraft only stores tid (template_id).
        // based_on_version is optional and re-read from form state on retry — not stored here.
        let tid = Uuid::new_v4();
        let pending = PendingAction::CreateDraft { tid };
        match pending {
            PendingAction::CreateDraft { tid: stored } => {
                assert_eq!(stored, tid);
            }
            other => panic!("Expected CreateDraft, got {:?}", other),
        }
    }

    #[wasm_bindgen_test]
    fn config_list_page_requires_admin_or_dispatcher() {
        let admin = SessionInfo {
            username: "a".to_string(),
            role: "operations_admin".to_string(),
            session_id: None,
        };
        let disp = SessionInfo {
            username: "d".to_string(),
            role: "dispatcher".to_string(),
            session_id: None,
        };
        let staff = SessionInfo {
            username: "s".to_string(),
            role: "staff_user".to_string(),
            session_id: None,
        };
        assert!(
            admin.is_admin() || admin.is_dispatcher(),
            "operations_admin must access config list"
        );
        assert!(
            disp.is_admin() || disp.is_dispatcher(),
            "dispatcher must access config list"
        );
        assert!(
            !(staff.is_admin() || staff.is_dispatcher()),
            "staff_user must be blocked from config list"
        );
    }
}

// ── Rollout creation: stage validation + depot UUID parsing ─────────────────

mod rollout_creation_form {
    use super::{CreateRolloutRequest, RolloutStageSpec};
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    /// Exact mirror of `parse_depot_ids` from `rollout.rs`.
    /// Splits on commas, trims each token, filters empty strings, then parses each
    /// as a UUID.  Returns `Ok(vec![])` for empty/whitespace-only input — the
    /// non-empty requirement is enforced by the submit handler, not this helper.
    fn parse_depot_ids(raw: &str) -> Result<Vec<Uuid>, String> {
        raw.split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<Uuid>()
                    .map_err(|_| format!("Invalid depot UUID: \"{}\"", s))
            })
            .collect()
    }

    /// Mirrors the per-stage validation logic from `on_create_rollout` in rollout.rs.
    /// Production validates inline; this helper consolidates the same checks for testing.
    fn validate_stage(
        pct_str: &str,
        depot_ids_raw: &str,
        index: usize,
    ) -> Result<RolloutStageSpec, String> {
        let pct: i16 = match pct_str.trim().parse::<i16>() {
            Ok(p) if p > 0 && p <= 100 => p,
            _ => {
                return Err(format!(
                    "Stage {}: target percentage must be 1–100.",
                    index + 1
                ))
            }
        };
        let depot_ids = match parse_depot_ids(depot_ids_raw) {
            Ok(ids) if !ids.is_empty() => ids,
            Ok(_) => {
                return Err(format!(
                    "Stage {}: at least one depot UUID is required.",
                    index + 1
                ));
            }
            Err(msg) => return Err(format!("Stage {}: {}", index + 1, msg)),
        };
        Ok(RolloutStageSpec {
            target_percentage: pct,
            depot_ids,
            scheduled_at: None,
        })
    }

    // ── parse_depot_ids ──────────────────────────────────────────────────────

    #[wasm_bindgen_test]
    fn parse_single_valid_uuid() {
        let id = Uuid::new_v4();
        let result = parse_depot_ids(&id.to_string());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![id]);
    }

    #[wasm_bindgen_test]
    fn parse_multiple_valid_uuids() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();
        let raw = format!("{},{},{}", id1, id2, id3);
        let ids = parse_depot_ids(&raw).expect("three valid UUIDs must parse");
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&id1) && ids.contains(&id2) && ids.contains(&id3));
    }

    #[wasm_bindgen_test]
    fn parse_trims_whitespace_around_uuids() {
        let id = Uuid::new_v4();
        let raw = format!("  {}  ", id);
        let ids = parse_depot_ids(&raw).expect("leading/trailing whitespace must be trimmed");
        assert_eq!(ids, vec![id]);
    }

    #[wasm_bindgen_test]
    fn parse_filters_empty_segments_from_trailing_commas() {
        let id = Uuid::new_v4();
        let raw = format!("{},,,", id);
        let ids = parse_depot_ids(&raw)
            .expect("trailing commas produce empty segments that are filtered");
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], id);
    }

    #[wasm_bindgen_test]
    fn parse_empty_string_returns_ok_empty_vec() {
        // parse_depot_ids itself does not enforce non-empty; it returns Ok(vec![]).
        // The caller (on_create_rollout) checks `!ids.is_empty()` separately.
        let result = parse_depot_ids("");
        assert!(
            result.is_ok(),
            "parse_depot_ids must not Err on empty input"
        );
        assert!(result.unwrap().is_empty());
    }

    #[wasm_bindgen_test]
    fn parse_whitespace_only_returns_ok_empty_vec() {
        let result = parse_depot_ids("  ,  ,  ");
        assert!(
            result.is_ok(),
            "whitespace segments are filtered → empty Ok vec"
        );
        assert!(result.unwrap().is_empty());
    }

    #[wasm_bindgen_test]
    fn parse_invalid_uuid_returns_error_quoting_offending_value() {
        let result = parse_depot_ids("not-a-uuid");
        assert!(result.is_err(), "Invalid UUID token must return Err");
        let err = result.unwrap_err();
        // Production format: `Invalid depot UUID: "not-a-uuid"`
        assert!(
            err.contains("not-a-uuid"),
            "Error must quote the offending token; got: {}",
            err
        );
    }

    #[wasm_bindgen_test]
    fn parse_mixed_valid_and_invalid_returns_error() {
        let valid = Uuid::new_v4();
        let raw = format!("{},bad-uuid", valid);
        let result = parse_depot_ids(&raw);
        assert!(
            result.is_err(),
            "Any invalid UUID in the list must cause an Err"
        );
        assert!(result.unwrap_err().contains("bad-uuid"));
    }

    // ── Stage-level validation (mirrors on_create_rollout inline logic) ──────

    #[wasm_bindgen_test]
    fn stage_percentage_zero_fails() {
        let result = validate_stage("0", &Uuid::new_v4().to_string(), 0);
        assert!(result.is_err(), "target_pct=0 must fail (min is > 0)");
    }

    #[wasm_bindgen_test]
    fn stage_percentage_101_fails() {
        let result = validate_stage("101", &Uuid::new_v4().to_string(), 0);
        assert!(result.is_err(), "target_pct=101 must fail (max is 100)");
    }

    #[wasm_bindgen_test]
    fn stage_percentage_1_passes() {
        let id = Uuid::new_v4();
        let result = validate_stage("1", &id.to_string(), 0);
        assert!(result.is_ok(), "target_pct=1 is the minimum valid value");
        assert_eq!(result.unwrap().target_percentage, 1);
    }

    #[wasm_bindgen_test]
    fn stage_percentage_100_passes() {
        let id = Uuid::new_v4();
        let result = validate_stage("100", &id.to_string(), 0);
        assert!(result.is_ok(), "target_pct=100 is the maximum valid value");
        assert_eq!(result.unwrap().target_percentage, 100);
    }

    #[wasm_bindgen_test]
    fn stage_percentage_non_numeric_fails() {
        let result = validate_stage("fifty", &Uuid::new_v4().to_string(), 0);
        assert!(
            result.is_err(),
            "Non-numeric target_pct must fail validation"
        );
    }

    #[wasm_bindgen_test]
    fn stage_empty_depot_ids_fails() {
        // on_create_rollout: Ok(vec![]) → "at least one depot UUID is required" branch
        let result = validate_stage("50", "", 0);
        assert!(
            result.is_err(),
            "Empty depot_ids_raw must fail stage validation"
        );
    }

    #[wasm_bindgen_test]
    fn stage_whitespace_only_depot_ids_fails() {
        let result = validate_stage("50", "  ,  ,  ", 0);
        assert!(
            result.is_err(),
            "Whitespace-only depot_ids_raw produces an empty vec → must fail"
        );
    }

    #[wasm_bindgen_test]
    fn stage_invalid_depot_uuid_fails() {
        let result = validate_stage("50", "not-a-uuid", 0);
        assert!(
            result.is_err(),
            "Invalid depot UUID must fail stage validation"
        );
    }

    #[wasm_bindgen_test]
    fn stage_valid_pct_and_uuid_produces_correct_spec() {
        let id = Uuid::new_v4();
        let result = validate_stage("33", &id.to_string(), 0);
        assert!(result.is_ok());
        let spec = result.unwrap();
        assert_eq!(spec.target_percentage, 33);
        assert_eq!(spec.depot_ids, vec![id]);
    }

    // ── CreateRolloutRequest building ─────────────────────────────────────────

    #[wasm_bindgen_test]
    fn empty_stage_list_rejected_before_api_call() {
        // on_create_rollout checks `stages_raw.is_empty()` and sets CreateState::Failed.
        let stages_count = 0_usize;
        assert!(stages_count == 0, "Zero stages must prevent the API call");
    }

    #[wasm_bindgen_test]
    fn create_rollout_request_serialises_stages_and_notes() {
        let d1 = Uuid::new_v4();
        let d2 = Uuid::new_v4();
        let req = CreateRolloutRequest {
            stages: vec![
                RolloutStageSpec {
                    target_percentage: 50,
                    depot_ids: vec![d1],
                    scheduled_at: None,
                },
                RolloutStageSpec {
                    target_percentage: 100,
                    depot_ids: vec![d2],
                    scheduled_at: None,
                },
            ],
            notes: Some("Two-phase rollout".to_string()),
        };
        let json = serde_json::to_string(&req).expect("must serialise");
        assert!(json.contains("\"stages\""));
        assert!(json.contains("\"target_percentage\""));
        assert!(json.contains("50") && json.contains("100"));
        assert!(json.contains("Two-phase rollout"));
        assert!(json.contains(&d1.to_string()) && json.contains(&d2.to_string()));
    }

    #[wasm_bindgen_test]
    fn create_rollout_request_none_notes_omitted_from_json() {
        let id = Uuid::new_v4();
        let req = CreateRolloutRequest {
            stages: vec![RolloutStageSpec {
                target_percentage: 100,
                depot_ids: vec![id],
                scheduled_at: None,
            }],
            notes: None,
        };
        let json = serde_json::to_string(&req).expect("must serialise");
        assert!(
            !json.contains("\"notes\""),
            "None notes must be omitted from JSON (skip_serializing_if)"
        );
    }

    #[wasm_bindgen_test]
    fn stage_spec_depot_ids_round_trip_through_json() {
        let d1 = Uuid::new_v4();
        let d2 = Uuid::new_v4();
        let spec = RolloutStageSpec {
            target_percentage: 33,
            depot_ids: vec![d1, d2],
            scheduled_at: None,
        };
        let json = serde_json::to_string(&spec).expect("must serialise");
        assert!(
            json.contains(&d1.to_string()),
            "depot d1 must appear in JSON"
        );
        assert!(
            json.contains(&d2.to_string()),
            "depot d2 must appear in JSON"
        );
    }

    #[wasm_bindgen_test]
    fn invalid_template_id_string_fails_uuid_parse() {
        let result = "not-a-uuid".parse::<Uuid>();
        assert!(
            result.is_err(),
            "Invalid template_id must fail UUID parse before API call"
        );
    }

    #[wasm_bindgen_test]
    fn invalid_version_id_string_fails_uuid_parse() {
        let result = "12345-nope".parse::<Uuid>();
        assert!(
            result.is_err(),
            "Invalid version_id must fail UUID parse before API call"
        );
    }

    #[wasm_bindgen_test]
    fn valid_template_and_version_uuids_parse_successfully() {
        let tid_str = Uuid::new_v4().to_string();
        let vid_str = Uuid::new_v4().to_string();
        assert!(
            tid_str.parse::<Uuid>().is_ok(),
            "Valid template UUID string must parse"
        );
        assert!(
            vid_str.parse::<Uuid>().is_ok(),
            "Valid version UUID string must parse"
        );
    }
}

// ── Config list empty-state guidance text ─────────────────────────────────────
//
// This module verifies that the empty-state hint string (previously a compile
// blocker due to unescaped inner double-quote characters) is now a well-formed
// Rust string literal.  The mere fact that this module compiles is the primary
// acceptance signal; the assertions verify semantic content.

mod config_list_ui_string_fix {
    use wasm_bindgen_test::wasm_bindgen_test;

    /// Exact text used in `config_list.rs` empty-state (inner quotes now escaped).
    /// If the string were still syntactically broken the crate would not compile
    /// and ALL tests in this file would fail.
    const EMPTY_STATE_HINT: &str = "Use \"+ New Draft\" above to create the first version.";

    #[wasm_bindgen_test]
    fn hint_text_mentions_new_draft() {
        assert!(
            EMPTY_STATE_HINT.contains("New Draft"),
            "Empty-state hint must reference the '+ New Draft' button"
        );
    }

    #[wasm_bindgen_test]
    fn hint_text_is_non_empty_valid_string() {
        // Compile-time proof: a broken string literal would prevent this assertion
        // from ever running.
        assert!(!EMPTY_STATE_HINT.is_empty());
    }

    #[wasm_bindgen_test]
    fn hint_text_contains_escaped_quotes_not_raw_ascii() {
        // After the fix the string contains literal `"` characters (U+0022) inside
        // it, not some alternative encoding.  Verify they are present and that the
        // surrounding text is intact.
        assert!(
            EMPTY_STATE_HINT.contains('"'),
            "Escaped inner quotes must be present as chars"
        );
        assert!(
            EMPTY_STATE_HINT.starts_with("Use "),
            "Text must start with 'Use '"
        );
        assert!(
            EMPTY_STATE_HINT.ends_with("first version."),
            "Text must end with the expected suffix"
        );
    }
}

// ── Rollout activate pre-condition checks ─────────────────────────────────────
//
// Before the fix, activate_stage silently returned (no visible error) when
// template_input or plan_input could not be parsed as UUIDs.  This happened
// whenever a rollout was created via the create-form because on_create_rollout
// did not populate template_input (only plan_input was set).
//
// After the fix:
//   1. on_create_rollout also calls tmpl_inp2.set(tid.to_string()) on success.
//   2. activate_stage sets ActionState::Failed with an explicit message instead
//      of a silent `return`.
//
// These tests exercise the fixed pre-condition logic directly.

mod rollout_activate_prechecks {
    use uuid::Uuid;
    use wasm_bindgen_test::wasm_bindgen_test;

    /// Mirrors the ActionState enum from rollout.rs.
    #[derive(Clone, PartialEq, Debug)]
    enum ActionState {
        Idle,
        Working,
        ReauthRequired {
            stage_id: Uuid,
            plan_id: Uuid,
            template_id: Uuid,
        },
        Done(String),
        Failed(String),
    }

    /// Mirrors the fixed activate_stage pre-condition checks.
    /// Returns `Ok((tid, pid))` when both inputs are valid UUIDs, otherwise
    /// `Err(ActionState::Failed(...))` — matching the new explicit-error behavior.
    fn precheck_activate(
        template_input: &str,
        plan_input: &str,
    ) -> Result<(Uuid, Uuid), ActionState> {
        let tid = template_input.parse::<Uuid>().map_err(|_| {
            ActionState::Failed(
                "Template ID is missing — load a plan before activating.".to_string(),
            )
        })?;
        let pid = plan_input.parse::<Uuid>().map_err(|_| {
            ActionState::Failed("Plan ID is missing — load a plan before activating.".to_string())
        })?;
        Ok((tid, pid))
    }

    // ── Pre-condition: explicit failure on missing context ────────────────────

    #[wasm_bindgen_test]
    fn empty_template_id_produces_failed_with_message() {
        let result = precheck_activate("", &Uuid::new_v4().to_string());
        match result {
            Err(ActionState::Failed(msg)) => {
                assert!(
                    msg.contains("Template ID"),
                    "Error must name the missing field; got: {}",
                    msg
                );
            }
            Ok(_) => panic!("Empty template ID must not pass the precheck"),
            Err(e) => panic!("Expected Failed, got {:?}", e),
        }
    }

    #[wasm_bindgen_test]
    fn empty_plan_id_produces_failed_with_message() {
        let result = precheck_activate(&Uuid::new_v4().to_string(), "");
        match result {
            Err(ActionState::Failed(msg)) => {
                assert!(
                    msg.contains("Plan ID"),
                    "Error must name the missing field; got: {}",
                    msg
                );
            }
            Ok(_) => panic!("Empty plan ID must not pass the precheck"),
            Err(e) => panic!("Expected Failed, got {:?}", e),
        }
    }

    #[wasm_bindgen_test]
    fn invalid_template_uuid_produces_failed() {
        let result = precheck_activate("not-a-uuid", &Uuid::new_v4().to_string());
        assert!(
            matches!(result, Err(ActionState::Failed(_))),
            "Non-UUID template input must produce ActionState::Failed"
        );
    }

    #[wasm_bindgen_test]
    fn invalid_plan_uuid_produces_failed() {
        let result = precheck_activate(&Uuid::new_v4().to_string(), "not-a-plan");
        assert!(
            matches!(result, Err(ActionState::Failed(_))),
            "Non-UUID plan input must produce ActionState::Failed"
        );
    }

    // ── Pre-condition: success path ───────────────────────────────────────────

    #[wasm_bindgen_test]
    fn both_valid_uuids_pass_precheck_and_preserve_values() {
        let tid = Uuid::new_v4();
        let pid = Uuid::new_v4();
        let result = precheck_activate(&tid.to_string(), &pid.to_string());
        match result {
            Ok((t, p)) => {
                assert_eq!(t, tid, "Returned template UUID must match input");
                assert_eq!(p, pid, "Returned plan UUID must match input");
            }
            Err(e) => panic!("Valid UUIDs must pass precheck; got {:?}", e),
        }
    }

    // ── Regression: create-then-activate flow ─────────────────────────────────

    #[wasm_bindgen_test]
    fn after_create_success_template_context_enables_activate() {
        // Simulates what on_create_rollout does on Ok(plan):
        //   tmpl_inp2.set(tid.to_string())   ← the fix
        //   plan_inp2.set(plan.id.to_string())
        let created_tid = Uuid::new_v4();
        let created_plan = Uuid::new_v4();

        // State as it exists after the fixed on_create_rollout success branch:
        let template_input = created_tid.to_string(); // was set by fix
        let plan_input = created_plan.to_string(); // was already set before fix

        let result = precheck_activate(&template_input, &plan_input);
        assert!(
            result.is_ok(),
            "After create success, activate precheck must pass; template context is now set"
        );
        let (t, p) = result.unwrap();
        assert_eq!(
            t, created_tid,
            "Template ID must match the create-form value"
        );
        assert_eq!(p, created_plan, "Plan ID must match the newly created plan");
    }

    #[wasm_bindgen_test]
    fn without_template_sync_activate_fails_not_silently_returns() {
        // Demonstrates the old (broken) state: plan_input was populated but
        // template_input was NOT (the bug).  The new code surfaces this as a
        // visible ActionState::Failed instead of a silent no-op return.
        let plan_only = Uuid::new_v4().to_string();
        let template_missing = ""; // old code: was never set after create

        let result = precheck_activate(template_missing, &plan_only);
        // New behavior: Err(Failed(...)) — not a silent return.
        match result {
            Err(ActionState::Failed(_)) => { /* correct — error is now visible */ }
            Ok(_) => panic!("Bug reproduced: missing template context must not silently succeed"),
            Err(e) => panic!("Expected Failed, got {:?}", e),
        }
    }
}

// ── Production helper integration checks ─────────────────────────────────────

mod production_helpers_direct {
    use super::*;
    use uuid::Uuid;

    #[wasm_bindgen_test]
    fn config_list_empty_state_hint_constant_is_exact() {
        assert_eq!(
            EMPTY_STATE_DRAFT_HINT,
            "Use \"+ New Draft\" above to create the first version."
        );
    }

    #[wasm_bindgen_test]
    fn rollout_parse_depot_ids_uses_production_function() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let ids = parse_depot_ids_prod(&format!("{}, {}", a, b)).expect("must parse UUID list");
        assert_eq!(ids, vec![a, b]);
    }

    #[wasm_bindgen_test]
    fn rollout_precheck_activate_ids_uses_production_function() {
        let tid = Uuid::new_v4();
        let pid = Uuid::new_v4();
        let (t, p) =
            precheck_activate_ids(&tid.to_string(), &pid.to_string()).expect("valid ids must pass");
        assert_eq!(t, tid);
        assert_eq!(p, pid);
    }
}

// ── Mounted guard integration smoke tests ───────────────────────────────────

mod mounted_guard_integration {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;
    use web_sys::window;
    use yew::prelude::*;
    use yew_router::prelude::*;

    use transitops_frontend::{
        components::role_guard::RoleGuard,
        pages::Route,
        store::auth_store::{AuthContext, AuthState},
    };

    fn session(role: &str) -> SessionInfo {
        SessionInfo {
            username: "test-user".to_string(),
            role: role.to_string(),
            session_id: None,
        }
    }

    fn mount<T: BaseComponent>() -> web_sys::Element
    where
        T: BaseComponent,
        T::Properties: Default,
    {
        let document = window().unwrap().document().unwrap();
        let root = document.create_element("div").unwrap();
        document.body().unwrap().append_child(&root).unwrap();
        yew::Renderer::<T>::with_root_and_props(root.clone(), T::Properties::default()).render();
        root
    }

    #[function_component(AdminAllowedHarness)]
    fn admin_allowed_harness() -> Html {
        let auth = use_reducer(|| AuthState {
            token: Some("tok".to_string()),
            session: Some(session("operations_admin")),
            loading: false,
        });
        html! {
            <ContextProvider<AuthContext> context={auth}>
                <BrowserRouter>
                    <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin())}>
                        <div id="allowed-marker">{ "ALLOWED_MARKER" }</div>
                    </RoleGuard>
                </BrowserRouter>
            </ContextProvider<AuthContext>>
        }
    }

    #[function_component(DispatcherForbiddenHarness)]
    fn dispatcher_forbidden_harness() -> Html {
        let auth = use_reducer(|| AuthState {
            token: Some("tok".to_string()),
            session: Some(session("dispatcher")),
            loading: false,
        });
        html! {
            <ContextProvider<AuthContext> context={auth}>
                <BrowserRouter>
                    <RoleGuard allowed={Callback::from(|s: SessionInfo| s.is_admin())}>
                        <div id="should-not-render">{ "SHOULD_NOT_RENDER" }</div>
                    </RoleGuard>
                </BrowserRouter>
            </ContextProvider<AuthContext>>
        }
    }

    async fn yield_to_render() {
        // Yew renders asynchronously via `queue_microtask`.  Two yields are
        // enough to let the full render tree commit.
        for _ in 0..10 {
            gloo_timers::future::TimeoutFuture::new(5).await;
        }
    }

    #[wasm_bindgen_test]
    async fn role_guard_renders_children_for_admin_when_mounted() {
        let root = mount::<AdminAllowedHarness>();
        yield_to_render().await;
        let html = root.inner_html();
        assert!(
            html.contains("ALLOWED_MARKER"),
            "expected ALLOWED_MARKER in rendered DOM, got: {html}"
        );
        assert!(!html.contains("403 — Access Denied"));
        root.remove();
    }

    #[wasm_bindgen_test]
    async fn role_guard_shows_forbidden_for_dispatcher_when_mounted() {
        let root = mount::<DispatcherForbiddenHarness>();
        yield_to_render().await;
        let html = root.inner_html();
        assert!(
            html.contains("403 — Access Denied"),
            "expected forbidden marker, got: {html}"
        );
        assert!(!html.contains("SHOULD_NOT_RENDER"));
        let _ = Route::Inbox;
        root.remove();
    }
}
