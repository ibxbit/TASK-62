# Test Coverage Audit

## Project Type Detection
- Declared type found: `fullstack` in `repo/README.md:3`.
- Inference fallback not required.

## Backend Endpoint Inventory
Resolved from Actix route registration in:
- `repo/src/main.rs:106`
- `repo/src/auth/mod.rs:11`
- `repo/src/ops/mod.rs:40`
- `repo/src/dispatcher/mod.rs:34`
- `repo/src/notifications/mod.rs:56`
- `repo/src/payments/mod.rs:52`
- `repo/src/reconciliation/mod.rs:30`
- `repo/src/reporting/mod.rs:48`
- `repo/src/alerting/mod.rs:43`
- `repo/src/audit/mod.rs:35`

Total unique endpoints (`METHOD + fully resolved PATH`): **122**

### Endpoints by module
- **Auth (4)**: `POST /auth/login`, `POST /auth/logout`, `GET /auth/session`, `POST /auth/reauth`
- **Ops (37)**: `/ops/routes*`, `/ops/trips*`, `/ops/calendars*`, `/ops/configs/*`
- **Dispatcher (15)**: `/dispatcher/trips*`, `/dispatcher/conflicts*`, `/dispatcher/monitor*`
- **Notifications (22)**: inbox, preferences, subscriptions, rules, announce, receipt, channels
- **Payments (16)**: transactions, callbacks, imports, refunds, compensation
- **Reconciliation (7)**: statements and runs
- **Reporting (14)**: metrics, schedules, runs
- **Alerting (5)**: list/stats/detail/ack/close
- **Audit (2)**: list/detail logs

## API Test Mapping Table

| Endpoint | Covered | Test type | Test files | Evidence |
|---|---|---|---|---|
| POST /auth/login | yes | true no-mock HTTP | `repo/API_tests/test_auth_api.py`, `repo/e2e/tests/login.spec.ts` | `TestLogin.test_admin_login_succeeds`; `Authentication flow > seeded admin can log in...` |
| POST /auth/logout | yes | true no-mock HTTP | `repo/API_tests/test_auth_api.py` | `TestLogout.test_logout_returns_200` |
| GET /auth/session | yes | true no-mock HTTP | `repo/API_tests/test_auth_api.py` | `TestSession.test_authenticated_session_returns_200` |
| POST /auth/reauth | yes | true no-mock HTTP | `repo/API_tests/test_auth_api.py`, `repo/API_tests/test_reauth_gated.py` | `TestReauth.test_reauth_with_correct_password_succeeds`; `_fresh_token_with_reauth` |
| GET /ops/routes | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py`, `repo/API_tests/test_rbac_api.py` | `TestOpsRoutesCrud.test_list_routes_happy_path`; `TestOpsWriteRbac.test_all_roles_can_read_routes` |
| POST /ops/routes | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py`, `repo/API_tests/test_rbac_api.py` | `TestOpsRoutesCrud.test_create_route_returns_201_with_id_and_code` |
| GET /ops/routes/{id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsRoutesCrud.test_get_route_returns_created_route` |
| PUT /ops/routes/{id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsRoutesCrud.test_update_route_persists_new_name` |
| DELETE /ops/routes/{id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py`, `repo/API_tests/test_rbac_api.py` | `TestOpsRoutesCrud.test_delete_route_removes_resource` |
| POST /ops/routes/{id}/publish | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py`, `repo/API_tests/test_reauth_gated.py` | `TestOpsRoutesCrud.test_publish_route_nonexistent_returns_404` |
| POST /ops/routes/{id}/unpublish | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsRoutesCrud.test_unpublish_route_nonexistent_returns_404` |
| POST /ops/routes/{id}/schedule | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsRoutesCrud.test_schedule_route_invalid_body_returns_400` |
| GET /ops/routes/{route_id}/stops | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsStopsCrud.test_list_stops_returns_list` |
| POST /ops/routes/{route_id}/stops | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsStopsCrud.test_create_stop_invalid_body_returns_400` |
| GET /ops/routes/{route_id}/stops/{stop_id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsStopsCrud.test_get_stop_nonexistent_returns_404` |
| PUT /ops/routes/{route_id}/stops/{stop_id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsStopsCrud.test_update_stop_nonexistent_returns_404` |
| DELETE /ops/routes/{route_id}/stops/{stop_id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsStopsCrud.test_delete_stop_nonexistent_is_idempotent` |
| GET /ops/trips | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsTripsCrud.test_list_trips_returns_paged_envelope` |
| POST /ops/trips | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsTripsCrud.test_create_trip_invalid_body_returns_400` |
| GET /ops/trips/{id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsTripsCrud.test_get_trip_nonexistent_returns_404` |
| PUT /ops/trips/{id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsTripsCrud.test_update_trip_nonexistent_returns_404` |
| DELETE /ops/trips/{id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsTripsCrud.test_delete_trip_nonexistent_returns_404_or_204` |
| POST /ops/trips/{id}/publish | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsTripsCrud.test_publish_trip_invalid_body_returns_400` |
| POST /ops/trips/{id}/unpublish | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsTripsCrud.test_unpublish_trip_invalid_body_returns_400` |
| POST /ops/trips/{id}/schedule | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsTripsCrud.test_schedule_trip_invalid_body_returns_400` |
| GET /ops/calendars | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsCalendarsCrud.test_list_calendars_returns_array` |
| POST /ops/calendars | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsCalendarsCrud.test_create_calendar_invalid_body_returns_400` |
| GET /ops/calendars/{id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsCalendarsCrud.test_get_calendar_nonexistent_returns_404` |
| PUT /ops/calendars/{id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsCalendarsCrud.test_update_calendar_nonexistent_returns_404` |
| DELETE /ops/calendars/{id} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsCalendarsCrud.test_delete_calendar_nonexistent_is_idempotent` |
| GET /ops/configs/{tid}/versions/diff | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsConfigVersions.test_diff_versions_missing_params_returns_400` |
| GET /ops/configs/{tid}/versions | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsConfigVersions.test_list_versions_returns_paged_envelope` |
| POST /ops/configs/{tid}/versions | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsConfigVersions.test_create_version_invalid_body_returns_400` |
| GET /ops/configs/{tid}/versions/{vid} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsConfigVersions.test_get_version_nonexistent_returns_404` |
| PUT /ops/configs/{tid}/versions/{vid} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsConfigVersions.test_update_version_nonexistent_returns_404` |
| POST /ops/configs/{tid}/versions/{vid}/publish | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py`, `repo/API_tests/test_rbac_api.py` | `TestPublishReauth.test_publish_without_reauth_returns_403` |
| POST /ops/configs/{tid}/versions/{vid}/unpublish | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py` | `TestUnpublishReauth.test_unpublish_without_reauth_returns_403` |
| POST /ops/configs/{tid}/versions/{vid}/schedule | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py` | `TestScheduleVersionReauth.test_schedule_without_reauth_returns_403` |
| POST /ops/configs/{tid}/versions/{vid}/rollout | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py` | `TestRolloutReauth.test_rollout_without_reauth_returns_403` |
| GET /ops/configs/{tid}/rollout/{pid} | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsConfigVersions.test_get_rollout_plan_nonexistent_returns_404` |
| POST /ops/configs/{tid}/rollout/{pid}/stages/{sid}/activate | yes | true no-mock HTTP | `repo/API_tests/test_ops_api.py` | `TestOpsConfigVersions.test_activate_rollout_stage_with_reauthed_admin` |
| PATCH /dispatcher/trips/{id} | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherTripLifecycle.test_patch_trip_nonexistent_returns_404` |
| POST /dispatcher/trips/{id}/assign | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherTripLifecycle.test_assign_driver_invalid_body_returns_400` |
| POST /dispatcher/trips/{id}/start | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherTripLifecycle.test_start_trip_nonexistent_returns_400` |
| POST /dispatcher/trips/{id}/complete | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherTripLifecycle.test_complete_trip_nonexistent_returns_400` |
| POST /dispatcher/trips/{id}/cancel | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherTripLifecycle.test_cancel_trip_nonexistent_returns_400` |
| GET /dispatcher/trips/{id}/conflicts | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherTripConflicts.test_get_trip_conflicts_returns_list` |
| POST /dispatcher/trips/{id}/check | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherTripConflicts.test_check_trip_conflicts_nonexistent_returns_404` |
| GET /dispatcher/conflicts | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherConflictManagement.test_list_conflicts_returns_array` |
| POST /dispatcher/conflicts/{id}/acknowledge | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherConflictManagement.test_acknowledge_conflict_invalid_body_returns_400` |
| POST /dispatcher/conflicts/{id}/resolve | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherConflictManagement.test_resolve_conflict_invalid_body_returns_400` |
| GET /dispatcher/monitor/dashboard | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherMonitor.test_dashboard_response_contract` |
| GET /dispatcher/monitor/upcoming | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherMonitor.test_upcoming_response_contract` |
| GET /dispatcher/monitor/active | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherMonitor.test_active_trips_returns_list` |
| GET /dispatcher/monitor/unassigned | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherMonitor.test_unassigned_response_contract` |
| POST /dispatcher/monitor/check-approaching | yes | true no-mock HTTP | `repo/API_tests/test_dispatcher_api.py` | `TestDispatcherMonitor.test_check_approaching_returns_200` |
| GET /notifications | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py`, `repo/e2e/tests/notifications.spec.ts` | `TestNotificationsList.test_list_returns_200` |
| GET /notifications/unread-count | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestUnreadCount.test_unread_count_response_contract` |
| POST /notifications/read-all | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py`, `repo/e2e/tests/notifications.spec.ts` | `TestMarkRead.test_read_all_returns_200` |
| GET /notifications/{id} | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestMarkRead.test_get_nonexistent_notification_returns_404` |
| POST /notifications/{id}/read | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestMarkRead.test_mark_single_nonexistent_returns_404` |
| POST /notifications/{id}/dismiss | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestMarkRead.test_dismiss_nonexistent_returns_404` |
| GET /notifications/preferences | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py`, `repo/API_tests/test_security.py` | `TestPreferences.test_get_preferences_returns_200` |
| PUT /notifications/preferences | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py`, `repo/API_tests/test_security.py` | `TestPreferences.test_enable_dnd_with_window_succeeds` |
| GET /notifications/subscriptions | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestSubscriptions.test_list_subscriptions_returns_200` |
| PUT /notifications/subscriptions | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestSubscriptions.test_update_subscriptions_with_event_types` |
| GET /notifications/rules | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestNotificationRules.test_list_rules_returns_200` |
| POST /notifications/rules | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py`, `repo/API_tests/test_security.py` | `TestNotificationRules.test_create_rule_succeeds` |
| GET /notifications/rules/{id} | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py`, `repo/API_tests/test_security.py` | `TestNotificationRules.test_get_nonexistent_rule_returns_404` |
| PUT /notifications/rules/{id} | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestNotificationRules.test_update_nonexistent_rule_returns_404` |
| DELETE /notifications/rules/{id} | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py`, `repo/API_tests/test_security.py` | `TestNotificationRules.test_delete_nonexistent_rule_returns_404` |
| POST /notifications/rules/{id}/toggle | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestNotificationRules.test_toggle_nonexistent_rule_returns_404` |
| POST /notifications/announce | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py`, `repo/API_tests/test_rbac_api.py` | `TestAnnounce.test_admin_can_announce` |
| POST /notifications/receipt | yes | true no-mock HTTP | `repo/API_tests/test_coverage_gaps.py` | `TestNotificationReceipt.test_receipt_valid_body_returns_promoted_count` |
| GET /notifications/channels | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestChannelPreferences.test_list_channel_prefs_returns_200` |
| PUT /notifications/channels/{channel} | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestChannelPreferences.test_upsert_email_channel_pref` |
| DELETE /notifications/channels/{channel} | yes | true no-mock HTTP | `repo/API_tests/test_notifications_api.py` | `TestChannelPreferences.test_delete_email_channel_pref` |
| GET /payments/transactions | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py`, `repo/API_tests/test_rbac_api.py` | `TestListTransactions.test_finance_can_list_transactions` |
| POST /payments/transactions | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py`, `repo/API_tests/test_rbac_api.py` | `TestCreateTransaction.test_finance_can_create_transaction` |
| GET /payments/transactions/{id} | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestListTransactions.test_get_nonexistent_transaction_returns_404` |
| POST /payments/callbacks/simulate | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py`, `repo/API_tests/test_security.py` | `TestCallbacks.test_simulate_callback_succeeds_for_real_transaction` |
| GET /payments/callbacks/{id} | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestCallbacks.test_get_nonexistent_callback_returns_404` |
| POST /payments/callbacks/{gateway} | yes | true no-mock HTTP | `repo/API_tests/test_security.py` | `TestCallbackSignatureVerification.test_callback_bad_signature_is_rejected` |
| GET /payments/imports | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestStatementImports.test_finance_can_list_imports` |
| POST /payments/imports | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestStatementImports.test_finance_can_upload_import` |
| GET /payments/imports/{id} | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestStatementImports.test_get_nonexistent_import_returns_404` |
| POST /payments/imports/{id}/process | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestStatementImports.test_process_nonexistent_import_returns_404` |
| GET /payments/refunds | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestListRefunds.test_finance_can_list_refunds` |
| POST /payments/refunds | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestCreateRefund.test_finance_can_create_refund` |
| GET /payments/refunds/{id} | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestListRefunds.test_get_nonexistent_refund_returns_404` |
| POST /payments/refunds/{id}/approve | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestListRefunds.test_approve_nonexistent_refund_returns_404` |
| POST /payments/refunds/{id}/process | yes | true no-mock HTTP | `repo/API_tests/test_payments_api.py` | `TestListRefunds.test_process_nonexistent_refund_returns_404` |
| GET /payments/compensation/jobs | yes | true no-mock HTTP | `repo/API_tests/test_coverage_gaps.py` | `TestPaymentsCompensation.test_list_compensation_jobs_returns_history` |
| POST /payments/compensation/trigger | yes | true no-mock HTTP | `repo/API_tests/test_coverage_gaps.py` | `TestPaymentsCompensation.test_trigger_compensation_returns_202_with_sweeps` |
| GET /reconciliation/statements | yes | true no-mock HTTP | `repo/API_tests/test_reconciliation_api.py` | `TestReconciliationStatements.test_list_statements_returns_array` |
| POST /reconciliation/statements | yes | true no-mock HTTP | `repo/API_tests/test_reconciliation_api.py` | `TestReconciliationStatements.test_upload_statement_invalid_body_returns_400` |
| GET /reconciliation/runs | yes | true no-mock HTTP | `repo/API_tests/test_reconciliation_api.py`, `repo/API_tests/test_rbac_api.py` | `TestReconciliationRbac.test_finance_can_list_runs` |
| POST /reconciliation/runs | yes | true no-mock HTTP | `repo/API_tests/test_rbac_api.py`, `repo/API_tests/test_reauth_gated.py` | `TestReconciliationRbac.test_finance_can_start_run` |
| GET /reconciliation/runs/{id} | yes | true no-mock HTTP | `repo/API_tests/test_reconciliation_api.py` | `TestReconciliationRunDetail.test_get_run_nonexistent_returns_404` |
| GET /reconciliation/runs/{id}/summary | yes | true no-mock HTTP | `repo/API_tests/test_reconciliation_api.py` | `TestReconciliationRunDetail.test_run_summary_nonexistent_returns_404` |
| GET /reconciliation/runs/{id}/items | yes | true no-mock HTTP | `repo/API_tests/test_reconciliation_api.py` | `TestReconciliationRunDetail.test_run_items_nonexistent_returns_empty_list` |
| GET /reporting/metrics | yes | true no-mock HTTP | `repo/API_tests/test_reporting_api.py`, `repo/e2e/tests/reporting.spec.ts` | `TestReportingMetrics.test_list_metrics_returns_array_with_builtin_fields` |
| POST /reporting/metrics | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py`, `repo/API_tests/test_rbac_api.py` | `TestMetricCreateReauth.test_create_metric_without_reauth_returns_403` |
| POST /reporting/metrics/compute | yes | true no-mock HTTP | `repo/API_tests/test_reporting_api.py` | `TestReportingMetrics.test_compute_metrics_invalid_body_returns_400` |
| GET /reporting/metrics/{id} | yes | true no-mock HTTP | `repo/API_tests/test_reporting_api.py` | `TestReportingMetrics.test_get_metric_nonexistent_returns_404` |
| PUT /reporting/metrics/{id} | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py` | `TestMetricUpdateReauth.test_update_metric_after_reauth_returns_404_for_nonexistent` |
| DELETE /reporting/metrics/{id} | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py` | `TestMetricDeleteReauth.test_delete_metric_after_reauth_returns_404_for_nonexistent` |
| GET /reporting/schedules | yes | true no-mock HTTP | `repo/API_tests/test_reporting_api.py` | `TestReportingSchedules.test_list_schedules_returns_array` |
| POST /reporting/schedules | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py` | `TestScheduleCreateReauth.test_create_schedule_without_reauth_returns_403` |
| GET /reporting/schedules/{id} | yes | true no-mock HTTP | `repo/API_tests/test_reporting_api.py` | `TestReportingSchedules.test_get_schedule_nonexistent_returns_404` |
| PUT /reporting/schedules/{id} | yes | true no-mock HTTP | `repo/API_tests/test_reporting_api.py`, `repo/API_tests/test_reauth_gated.py` | `TestReportingSchedules.test_update_schedule_nonexistent_returns_404` |
| DELETE /reporting/schedules/{id} | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py` | `TestScheduleDeleteReauth.test_delete_schedule_after_reauth_returns_404_for_nonexistent` |
| POST /reporting/schedules/{id}/trigger | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py` | `TestTriggerRunReauth.test_trigger_run_after_reauth_returns_404_for_nonexistent` |
| GET /reporting/runs | yes | true no-mock HTTP | `repo/API_tests/test_reporting_api.py` | `TestReportingRuns.test_list_runs_returns_array` |
| GET /reporting/runs/{id} | yes | true no-mock HTTP | `repo/API_tests/test_reporting_api.py` | `TestReportingRuns.test_get_run_nonexistent_returns_404` |
| GET /reporting/runs/{id}/export | yes | true no-mock HTTP | `repo/API_tests/test_reauth_gated.py` | `TestExportRunReauth.test_export_run_after_reauth_returns_404_for_nonexistent` |
| GET /alerts | yes | true no-mock HTTP | `repo/API_tests/test_alerting_api.py`, `repo/e2e/tests/alerts.spec.ts` | `TestListAlerts.test_admin_list_returns_200` |
| GET /alerts/stats | yes | true no-mock HTTP | `repo/API_tests/test_alerting_api.py` | `TestAlertStats.test_stats_returns_full_contract` |
| GET /alerts/{id} | yes | true no-mock HTTP | `repo/API_tests/test_alerting_api.py` | `TestGetAlert.test_nonexistent_alert_returns_404` |
| POST /alerts/{id}/acknowledge | yes | true no-mock HTTP | `repo/API_tests/test_alerting_api.py`, `repo/API_tests/test_rbac_api.py` | `TestAcknowledgeAlert.test_acknowledge_nonexistent_returns_404` |
| POST /alerts/{id}/close | yes | true no-mock HTTP | `repo/API_tests/test_alerting_api.py` | `TestCloseAlert.test_close_nonexistent_returns_404` |
| GET /audit/logs | yes | true no-mock HTTP | `repo/API_tests/test_rbac_api.py` | `TestAuditRbac.test_admin_can_read_audit_logs` |
| GET /audit/logs/{id} | yes | true no-mock HTTP | `repo/API_tests/test_coverage_gaps.py` | `TestAuditLogDetail.test_get_audit_log_nonexistent_returns_404` |

## API Test Classification

### 1) True No-Mock HTTP
- `repo/API_tests/*.py` (all API test modules) use real HTTP (`requests`) against `API_URL` through fixture `api()` in `repo/API_tests/conftest.py:303`.
- `repo/e2e/tests/*.spec.ts` use Playwright + real SPA + reverse proxy + real API (`repo/e2e/playwright.config.ts:11`, `repo/frontend/nginx.conf:24`).

### 2) HTTP with Mocking
- **None detected** by static inspection in API/E2E test suites.

### 3) Non-HTTP (unit/integration without HTTP)
- Backend Python unit tests: `repo/unit_tests/*.py` (pure replicated logic).
- Backend Rust tests: `repo/tests/*.rs` (direct library calls and DB-level integration, no HTTP route traversal).
- Frontend Rust wasm tests: `repo/frontend/tests/*.rs` (component/store/type logic and contracts, no backend HTTP transport invocation by test harness).

## Mock Detection

Searched for explicit mocking patterns (`jest.mock`, `vi.mock`, `sinon.stub`, monkeypatch/patch in API tests):
- No explicit HTTP-layer mocking in `repo/API_tests/*.py`.
- No explicit Playwright network stubbing in `repo/e2e/tests/*.spec.ts`.

Bypass-HTTP observations (not mocks, but relevant to classification):
- Direct backend function calls in Rust tests, e.g. `transitops_backend::notifications::bus::check_duplicate` in `repo/tests/idempotency.rs:163`.
- Pure re-implementations in Python unit tests, e.g. local `is_in_dnd_window()` in `repo/unit_tests/test_dnd_logic.py:19` instead of importing Rust code.

## Coverage Summary
- Total endpoints: **122**
- Endpoints with HTTP tests: **122**
- Endpoints with true no-mock HTTP tests: **122**
- HTTP coverage: **100.0%**
- True API coverage: **100.0%**

## Unit Test Summary

### Backend Unit Tests
Test files:
- `repo/unit_tests/test_dnd_logic.py`
- `repo/unit_tests/test_alert_severity.py`
- `repo/unit_tests/test_reconciliation_logic.py`
- `repo/unit_tests/test_signature_logic.py`
- `repo/tests/alert_dedup.rs`
- `repo/tests/dnd_edge_cases.rs`
- `repo/tests/idempotency.rs`
- `repo/tests/offline.rs`
- `repo/tests/reconciliation.rs`
- `repo/tests/replay_attack.rs`

Modules covered (directly or mirrored):
- **Controllers/handlers via HTTP**: auth, ops, dispatcher, notifications, payments, reconciliation, reporting, alerts, audit (`repo/API_tests/*.py`).
- **Services/domain**: alert detector (`repo/tests/alert_dedup.rs`), reconciliation discrepancy logic (`repo/tests/reconciliation.rs`), signature/replay (`repo/tests/replay_attack.rs`), notifications bus logic (`repo/tests/dnd_edge_cases.rs`, `repo/tests/idempotency.rs`).
- **Repository/DB boundaries**: several Rust tests hit real DB tables directly (`repo/tests/idempotency.rs:151`, `repo/tests/alert_dedup.rs:171`).
- **Auth/guards/middleware behavior**: strongly covered at API level (`repo/API_tests/test_rbac_api.py`, `repo/API_tests/test_reauth_gated.py`), not isolated as direct unit tests.

Important backend modules not tested in isolated unit style:
- `repo/src/auth/password.rs` (no direct unit test file found)
- `repo/src/crypto/mod.rs` (no direct unit test file found)
- `repo/src/reporting/export.rs` (covered via endpoint tests, not isolated logic unit tests)
- `repo/src/ops/diff.rs` (no dedicated direct unit test file found)

### Frontend Unit Tests
Frontend test files detected:
- `repo/frontend/tests/component_states.spec.rs`
- `repo/frontend/tests/notification_logic.spec.rs`
- `repo/frontend/tests/service_contracts.spec.rs`
- `repo/frontend/tests/dispatcher_workflows.spec.rs`
- `repo/frontend/tests/inbox_panel.spec.rs`
- `repo/frontend/tests/api_service.spec.rs`
- `repo/frontend/tests/role_guard.spec.rs`

Framework/tools detected:
- `wasm-bindgen-test` (`repo/frontend/Cargo.toml:34`)
- Rust/WASM browser test harness usage in files (`use wasm_bindgen_test::*;` and `wasm_bindgen_test_configure!(run_in_browser)`)

Components/modules covered:
- Auth role logic: `types/auth::SessionInfo` (`repo/frontend/tests/role_guard.spec.rs`, `repo/frontend/tests/component_states.spec.rs`)
- Notification domain + reducer: `types/notification`, `store/notification_store` (`repo/frontend/tests/notification_logic.spec.rs`, `repo/frontend/tests/inbox_panel.spec.rs`)
- Wire contracts/types: auth/ops/reporting/alerting/notification payload contracts (`repo/frontend/tests/service_contracts.spec.rs`, `repo/frontend/tests/api_service.spec.rs`)
- Dispatcher workflow domain type: `types/ops::TripConflict` (`repo/frontend/tests/dispatcher_workflows.spec.rs`)

Important frontend components/modules not tested directly:
- `repo/frontend/src/components/nav.rs`
- `repo/frontend/src/components/notification_card.rs`
- `repo/frontend/src/components/notification_badge.rs`
- `repo/frontend/src/components/reauth_prompt.rs`
- `repo/frontend/src/pages/login.rs`
- `repo/frontend/src/services/auth_service.rs`, `repo/frontend/src/services/finance_service.rs`, `repo/frontend/src/services/ops_service.rs`, `repo/frontend/src/services/reporting_service.rs`, `repo/frontend/src/services/alerting_service.rs`

**Mandatory verdict: Frontend unit tests: PRESENT**

### Cross-Layer Observation
- Backend test volume is significantly larger than frontend unit depth.
- Frontend is **not untested** (tests are present and real), but test depth is more contract/state focused than full component rendering coverage.

## API Observability Check
- Endpoint visibility: **strong** (explicit method/path strings in API tests, e.g. `api("POST", "/payments/transactions", ...)` in `repo/API_tests/test_payments_api.py:39`).
- Request input visibility: **strong** (JSON/query/headers explicitly set in tests).
- Response content visibility: **strong** for most suites (assertions on status + keys/values), moderate in a few places where only status is asserted.

## Tests Check
- Success paths: present across all domains.
- Failure paths: present (401/403/404/400 etc.) across all domains.
- Edge cases: present (idempotency, replay windows, DND boundaries, role matrix, reauth gates).
- Validation/auth/permissions: strongly covered.
- Integration boundaries: strong API + DB-backed checks, plus browser E2E.
- Over-mocking risk: low (no mocking found in API/E2E paths).
- `run_tests.sh`: Docker-contained orchestration confirmed (`repo/run_tests.sh:171`, `repo/run_tests.sh:180`, `repo/run_tests.sh:208`) → **OK**.

## Test Quality & Sufficiency
- API-level sufficiency is high: complete endpoint-level HTTP coverage with real transport and real backend handlers.
- Key weakness: part of Python "unit" suite validates replicated logic, not production Rust functions, reducing true unit confidence for those areas.
- Additional weakness: several frontend UI modules/components are not directly tested despite existing frontend unit suite.

## End-to-End Expectations
- Fullstack expectation (real FE ↔ BE) is met by Playwright E2E: `repo/e2e/tests/login.spec.ts`, `repo/e2e/tests/notifications.spec.ts`, `repo/e2e/tests/alerts.spec.ts`, `repo/e2e/tests/reporting.spec.ts`.

## Test Coverage Score (0-100)
**94/100**

## Score Rationale
- +40: full endpoint inventory covered by HTTP tests (122/122).
- +25: true no-mock API coverage present for all endpoints.
- +15: broad auth/RBAC/validation/edge-case coverage including reauth + security.
- +8: fullstack E2E FE↔BE flows exist.
- -6: some unit suites are mirrored logic rather than direct production-module execution.
- -2: frontend unit tests do not directly cover several key UI/service modules.

## Key Gaps
- Python unit tests (`repo/unit_tests/*.py`) mirror Rust behavior instead of testing Rust units directly.
- Some frontend components/services remain untested directly (navigation, prompt, notification card/badge, several page/service modules).

## Confidence & Assumptions
- Confidence: **high** for endpoint inventory and HTTP coverage; **medium-high** for qualitative sufficiency judgments.
- Static-only constraints honored: no tests/build/scripts executed.
- Assumed Actix route tree from compile-time registration is authoritative for endpoint inventory.

---

# README Audit

## README Location
- Found at required location: `repo/README.md`.

## Hard Gate Evaluation

### Formatting
- PASS: structured Markdown, clear sections/tables/code fences (`repo/README.md`).

### Startup Instructions
- PASS (fullstack/backend gate): includes `docker-compose up` explicitly (`repo/README.md:12`).
- Also includes `docker compose up` variant (`repo/README.md:18`).

### Access Method
- PASS: URL + ports documented for frontend/API/DB (`repo/README.md:50`).

### Verification Method
- PASS: concrete backend curl flow and web UI flow provided (`repo/README.md:82`, `repo/README.md:174`).

### Environment Rules (Docker-contained)
- PASS: Docker-first instructions; no required `npm install`, `pip install`, `apt-get`, or manual DB setup documented.

### Demo Credentials (auth present)
- PASS: credentials include username/email/password and all listed roles (`repo/README.md:71`).

## Engineering Quality
- Tech stack clarity: strong (Rust/Actix/Postgres + Yew/WASM stated clearly).
- Architecture explanation: strong (service addresses, reverse proxy behavior, test-stack isolation).
- Testing instructions: strong and structured (`./run_tests.sh` categories with CI notes).
- Security/roles: good (RBAC verification steps + role table + credentials warning).
- Workflow quality: good (startup, verification, teardown, test execution all present).
- Presentation quality: high (consistent headings, tables, examples, expected outputs).

## High Priority Issues
- None.

## Medium Priority Issues
- `docker-compose up` by itself may not start frontend unless profile/service selection is understood; README documents this, but startup path could be simplified to a single explicit fullstack command to reduce ambiguity (`repo/README.md:24`, `repo/README.md:179`).

## Low Priority Issues
- README is long; quick-start summary block at top could improve scan speed for first-time reviewers.

## Hard Gate Failures
- None.

## README Verdict
**PASS**

---

## Final Verdicts
- **Test Coverage Audit Verdict:** PASS with notable quality caveats (mirror-unit pattern + partial frontend direct-module depth).
- **README Audit Verdict:** PASS.
