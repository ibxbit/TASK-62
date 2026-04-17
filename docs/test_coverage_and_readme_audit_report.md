# Test Coverage Audit

## Project Type Detection
- Declared project type: `fullstack` (`repo/README.md:3`).

## Backend Endpoint Inventory
- Total endpoints: **122**
- Endpoint sources: `repo/src/*/mod.rs` via `repo/src/main.rs:106`-`repo/src/main.rs:114`.

## API Test Mapping Table
| Endpoint | covered | test type | test files | evidence |
|---|---|---|---|---|
| `DELETE /notifications/channels/{channel}` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:351` TestChannelPreferences.test_delete_email_channel_pref |
| `DELETE /notifications/rules/{id}` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py, API_tests/test_security.py` | `API_tests/test_notifications_api.py:256` TestNotificationRules.test_delete_nonexistent_rule_returns_404 |
| `DELETE /ops/calendars/{id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:298` TestOpsCalendarsCrud.test_delete_calendar_nonexistent_is_idempotent |
| `DELETE /ops/routes/{id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py, API_tests/test_rbac_api.py` | `API_tests/test_ops_api.py:84` TestOpsRoutesCrud.test_delete_route_removes_resource |
| `DELETE /ops/routes/{route_id}/stops/{stop_id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:216` TestOpsStopsCrud.test_delete_stop_nonexistent_is_idempotent |
| `DELETE /ops/trips/{id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:247` TestOpsTripsCrud.test_delete_trip_nonexistent_returns_404_or_204 |
| `DELETE /reporting/metrics/{id}` | yes | true no-mock HTTP | `API_tests/test_reauth_gated.py` | `API_tests/test_reauth_gated.py:241` TestMetricDeleteReauth.test_delete_metric_without_reauth_returns_403 |
| `DELETE /reporting/schedules/{id}` | yes | true no-mock HTTP | `API_tests/test_reauth_gated.py` | `API_tests/test_reauth_gated.py:271` TestScheduleDeleteReauth.test_delete_schedule_without_reauth_returns_403 |
| `GET /alerts` | yes | true no-mock HTTP | `API_tests/test_alerting_api.py, API_tests/test_rbac_api.py` | `API_tests/test_alerting_api.py:36` TestListAlerts.test_admin_list_returns_200 |
| `GET /alerts/stats` | yes | true no-mock HTTP | `API_tests/test_alerting_api.py` | `API_tests/test_alerting_api.py:117` TestAlertStats.test_stats_returns_full_contract |
| `GET /alerts/{id}` | yes | true no-mock HTTP | `API_tests/test_alerting_api.py` | `API_tests/test_alerting_api.py:148` TestGetAlert.test_nonexistent_alert_returns_404 |
| `GET /audit/logs` | yes | true no-mock HTTP | `API_tests/test_rbac_api.py` | `API_tests/test_rbac_api.py:47` TestAuditRbac.test_admin_can_read_audit_logs |
| `GET /audit/logs/{id}` | yes | true no-mock HTTP | `API_tests/test_coverage_gaps.py` | `API_tests/test_coverage_gaps.py:97` TestAuditLogDetail.test_get_audit_log_nonexistent_returns_404 |
| `GET /auth/session` | yes | true no-mock HTTP | `API_tests/test_auth_api.py` | `API_tests/test_auth_api.py:91` TestSession.test_authenticated_session_returns_200 |
| `GET /dispatcher/conflicts` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:102` TestDispatcherConflictManagement.test_list_conflicts_returns_array |
| `GET /dispatcher/monitor/active` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:169` TestDispatcherMonitor.test_active_trips_returns_list |
| `GET /dispatcher/monitor/dashboard` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:134` TestDispatcherMonitor.test_dashboard_response_contract |
| `GET /dispatcher/monitor/unassigned` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:174` TestDispatcherMonitor.test_unassigned_response_contract |
| `GET /dispatcher/monitor/upcoming` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:153` TestDispatcherMonitor.test_upcoming_response_contract |
| `GET /dispatcher/trips/{id}/conflicts` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:81` TestDispatcherTripConflicts.test_get_trip_conflicts_returns_list |
| `GET /notifications` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:41` TestNotificationsList.test_list_returns_200 |
| `GET /notifications/channels` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:331` TestChannelPreferences.test_list_channel_prefs_returns_200 |
| `GET /notifications/preferences` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py, API_tests/test_security.py` | `API_tests/test_notifications_api.py:129` TestPreferences.test_get_preferences_returns_200 |
| `GET /notifications/rules` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:206` TestNotificationRules.test_list_rules_returns_200 |
| `GET /notifications/rules/{id}` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py, API_tests/test_security.py` | `API_tests/test_notifications_api.py:245` TestNotificationRules.test_get_nonexistent_rule_returns_404 |
| `GET /notifications/subscriptions` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:174` TestSubscriptions.test_list_subscriptions_returns_200 |
| `GET /notifications/unread-count` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:87` TestUnreadCount.test_unread_count_response_contract |
| `GET /notifications/{id}` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:120` TestMarkRead.test_get_nonexistent_notification_returns_404 |
| `GET /ops/calendars` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:279` TestOpsCalendarsCrud.test_list_calendars_returns_array |
| `GET /ops/calendars/{id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:289` TestOpsCalendarsCrud.test_get_calendar_nonexistent_returns_404 |
| `GET /ops/configs/{tid}/rollout/{pid}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:343` TestOpsConfigVersions.test_get_rollout_plan_nonexistent_returns_404 |
| `GET /ops/configs/{tid}/versions` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:316` TestOpsConfigVersions.test_list_versions_returns_paged_envelope |
| `GET /ops/configs/{tid}/versions/diff` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:339` TestOpsConfigVersions.test_diff_versions_missing_params_returns_400 |
| `GET /ops/configs/{tid}/versions/{vid}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:320` TestOpsConfigVersions.test_get_version_nonexistent_returns_404 |
| `GET /ops/routes` | yes | true no-mock HTTP | `API_tests/test_ops_api.py, API_tests/test_rbac_api.py` | `API_tests/test_ops_api.py:148` TestOpsRoutesCrud.test_list_routes_happy_path |
| `GET /ops/routes/{id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:64` TestOpsRoutesCrud.test_get_route_returns_created_route |
| `GET /ops/routes/{route_id}/stops` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:171` TestOpsStopsCrud.test_list_stops_returns_list |
| `GET /ops/routes/{route_id}/stops/{stop_id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:204` TestOpsStopsCrud.test_get_stop_nonexistent_returns_404 |
| `GET /ops/trips` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:227` TestOpsTripsCrud.test_list_trips_returns_paged_envelope |
| `GET /ops/trips/{id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:238` TestOpsTripsCrud.test_get_trip_nonexistent_returns_404 |
| `GET /payments/callbacks/{id}` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:319` TestCallbacks.test_get_nonexistent_callback_returns_404 |
| `GET /payments/compensation/jobs` | yes | true no-mock HTTP | `API_tests/test_coverage_gaps.py` | `API_tests/test_coverage_gaps.py:57` TestPaymentsCompensation.test_list_compensation_jobs_returns_history |
| `GET /payments/imports` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:273` TestStatementImports.test_finance_can_list_imports |
| `GET /payments/imports/{id}` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:281` TestStatementImports.test_get_nonexistent_import_returns_404 |
| `GET /payments/refunds` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:200` TestListRefunds.test_finance_can_list_refunds |
| `GET /payments/refunds/{id}` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:216` TestListRefunds.test_get_nonexistent_refund_returns_404 |
| `GET /payments/transactions` | yes | true no-mock HTTP | `API_tests/test_payments_api.py, API_tests/test_rbac_api.py` | `API_tests/test_payments_api.py:106` TestListTransactions.test_finance_can_list_transactions |
| `GET /payments/transactions/{id}` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:131` TestListTransactions.test_get_nonexistent_transaction_returns_404 |
| `GET /reconciliation/runs` | yes | true no-mock HTTP | `API_tests/test_rbac_api.py` | `API_tests/test_rbac_api.py:253` TestReconciliationRbac.test_finance_can_list_runs |
| `GET /reconciliation/runs/{id}` | yes | true no-mock HTTP | `API_tests/test_reconciliation_api.py` | `API_tests/test_reconciliation_api.py:58` TestReconciliationRunDetail.test_get_run_nonexistent_returns_404 |
| `GET /reconciliation/runs/{id}/items` | yes | true no-mock HTTP | `API_tests/test_reconciliation_api.py` | `API_tests/test_reconciliation_api.py:69` TestReconciliationRunDetail.test_run_items_nonexistent_returns_empty_list |
| `GET /reconciliation/runs/{id}/summary` | yes | true no-mock HTTP | `API_tests/test_reconciliation_api.py` | `API_tests/test_reconciliation_api.py:63` TestReconciliationRunDetail.test_run_summary_nonexistent_returns_404 |
| `GET /reconciliation/statements` | yes | true no-mock HTTP | `API_tests/test_reconciliation_api.py` | `API_tests/test_reconciliation_api.py:28` TestReconciliationStatements.test_list_statements_returns_array |
| `GET /reporting/metrics` | yes | true no-mock HTTP | `API_tests/test_rbac_api.py, API_tests/test_reporting_api.py` | `API_tests/test_rbac_api.py:285` TestReportingRbac.test_admin_can_read_reporting |
| `GET /reporting/metrics/{id}` | yes | true no-mock HTTP | `API_tests/test_reporting_api.py` | `API_tests/test_reporting_api.py:35` TestReportingMetrics.test_get_metric_nonexistent_returns_404 |
| `GET /reporting/runs` | yes | true no-mock HTTP | `API_tests/test_reporting_api.py` | `API_tests/test_reporting_api.py:93` TestReportingRuns.test_list_runs_returns_array |
| `GET /reporting/runs/{id}` | yes | true no-mock HTTP | `API_tests/test_reporting_api.py` | `API_tests/test_reporting_api.py:98` TestReportingRuns.test_get_run_nonexistent_returns_404 |
| `GET /reporting/runs/{id}/export` | yes | true no-mock HTTP | `API_tests/test_reauth_gated.py` | `API_tests/test_reauth_gated.py:297` TestExportRunReauth.test_export_run_without_reauth_returns_403 |
| `GET /reporting/schedules` | yes | true no-mock HTTP | `API_tests/test_reporting_api.py` | `API_tests/test_reporting_api.py:59` TestReportingSchedules.test_list_schedules_returns_array |
| `GET /reporting/schedules/{id}` | yes | true no-mock HTTP | `API_tests/test_reporting_api.py` | `API_tests/test_reporting_api.py:64` TestReportingSchedules.test_get_schedule_nonexistent_returns_404 |
| `PATCH /dispatcher/trips/{id}` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:38` TestDispatcherTripLifecycle.test_patch_trip_nonexistent_returns_404 |
| `POST /alerts/{id}/acknowledge` | yes | true no-mock HTTP | `API_tests/test_alerting_api.py, API_tests/test_rbac_api.py` | `API_tests/test_alerting_api.py:169` TestAcknowledgeAlert.test_acknowledge_nonexistent_returns_404 |
| `POST /alerts/{id}/close` | yes | true no-mock HTTP | `API_tests/test_alerting_api.py` | `API_tests/test_alerting_api.py:212` TestCloseAlert.test_close_nonexistent_returns_404 |
| `POST /auth/login` | yes | true no-mock HTTP | `API_tests/test_auth_api.py, API_tests/test_reauth_gated.py` | `API_tests/test_auth_api.py:29` TestLogin.test_admin_login_succeeds |
| `POST /auth/logout` | yes | true no-mock HTTP | `API_tests/test_auth_api.py` | `API_tests/test_auth_api.py:124` TestLogout.test_logout_returns_200 |
| `POST /auth/reauth` | yes | true no-mock HTTP | `API_tests/test_auth_api.py, API_tests/test_reauth_gated.py` | `API_tests/test_auth_api.py:146` TestReauth.test_reauth_with_correct_password_succeeds |
| `POST /dispatcher/conflicts/{id}/acknowledge` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:113` TestDispatcherConflictManagement.test_acknowledge_conflict_invalid_body_returns_400 |
| `POST /dispatcher/conflicts/{id}/resolve` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:118` TestDispatcherConflictManagement.test_resolve_conflict_invalid_body_returns_400 |
| `POST /dispatcher/monitor/check-approaching` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:181` TestDispatcherMonitor.test_check_approaching_returns_200 |
| `POST /dispatcher/trips/{id}/assign` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:44` TestDispatcherTripLifecycle.test_assign_driver_invalid_body_returns_400 |
| `POST /dispatcher/trips/{id}/cancel` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:61` TestDispatcherTripLifecycle.test_cancel_trip_nonexistent_returns_400 |
| `POST /dispatcher/trips/{id}/check` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:89` TestDispatcherTripConflicts.test_check_trip_conflicts_nonexistent_returns_404 |
| `POST /dispatcher/trips/{id}/complete` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:56` TestDispatcherTripLifecycle.test_complete_trip_nonexistent_returns_400 |
| `POST /dispatcher/trips/{id}/start` | yes | true no-mock HTTP | `API_tests/test_dispatcher_api.py` | `API_tests/test_dispatcher_api.py:51` TestDispatcherTripLifecycle.test_start_trip_nonexistent_returns_400 |
| `POST /notifications/announce` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py, API_tests/test_rbac_api.py` | `API_tests/test_notifications_api.py:292` TestAnnounce.test_admin_can_announce |
| `POST /notifications/read-all` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:102` TestMarkRead.test_read_all_returns_200 |
| `POST /notifications/receipt` | yes | true no-mock HTTP | `API_tests/test_coverage_gaps.py` | `API_tests/test_coverage_gaps.py:31` TestNotificationReceipt.test_receipt_valid_body_returns_promoted_count |
| `POST /notifications/rules` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py, API_tests/test_security.py` | `API_tests/test_notifications_api.py:218` TestNotificationRules.test_create_rule_succeeds |
| `POST /notifications/rules/{id}/toggle` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:261` TestNotificationRules.test_toggle_nonexistent_rule_returns_404 |
| `POST /notifications/{id}/dismiss` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:115` TestMarkRead.test_dismiss_nonexistent_returns_404 |
| `POST /notifications/{id}/read` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:110` TestMarkRead.test_mark_single_nonexistent_returns_404 |
| `POST /ops/calendars` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:284` TestOpsCalendarsCrud.test_create_calendar_invalid_body_returns_400 |
| `POST /ops/configs/{tid}/versions` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:333` TestOpsConfigVersions.test_create_version_invalid_body_returns_400 |
| `POST /ops/configs/{tid}/versions/{vid}/publish` | yes | true no-mock HTTP | `API_tests/test_rbac_api.py` | `API_tests/test_rbac_api.py:169` TestOpsConfigRbac.test_dispatcher_cannot_publish_config_version |
| `POST /ops/configs/{tid}/versions/{vid}/rollout` | yes | true no-mock HTTP | `API_tests/test_reauth_gated.py` | `API_tests/test_reauth_gated.py:170` TestRolloutReauth.test_rollout_without_reauth_returns_403 |
| `POST /ops/configs/{tid}/versions/{vid}/schedule` | yes | true no-mock HTTP | `API_tests/test_reauth_gated.py` | `API_tests/test_reauth_gated.py:150` TestScheduleVersionReauth.test_schedule_without_reauth_returns_403 |
| `POST /ops/configs/{tid}/versions/{vid}/unpublish` | yes | true no-mock HTTP | `API_tests/test_reauth_gated.py` | `API_tests/test_reauth_gated.py:131` TestUnpublishReauth.test_unpublish_without_reauth_returns_403 |
| `POST /ops/routes` | yes | true no-mock HTTP | `API_tests/test_ops_api.py, API_tests/test_rbac_api.py` | `API_tests/test_ops_api.py:51` TestOpsRoutesCrud.test_create_route_returns_201_with_id_and_code |
| `POST /ops/routes/{id}/publish` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:112` TestOpsRoutesCrud.test_finance_cannot_publish_route |
| `POST /ops/routes/{id}/schedule` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:142` TestOpsRoutesCrud.test_schedule_route_invalid_body_returns_400 |
| `POST /ops/routes/{id}/unpublish` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:137` TestOpsRoutesCrud.test_unpublish_route_nonexistent_returns_404 |
| `POST /ops/routes/{route_id}/stops` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:187` TestOpsStopsCrud.test_create_stop_invalid_body_returns_400 |
| `POST /ops/trips` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:232` TestOpsTripsCrud.test_create_trip_invalid_body_returns_400 |
| `POST /ops/trips/{id}/publish` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:251` TestOpsTripsCrud.test_publish_trip_invalid_body_returns_400 |
| `POST /ops/trips/{id}/schedule` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:259` TestOpsTripsCrud.test_schedule_trip_invalid_body_returns_400 |
| `POST /ops/trips/{id}/unpublish` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:255` TestOpsTripsCrud.test_unpublish_trip_invalid_body_returns_400 |
| `POST /payments/callbacks/simulate` | yes | true no-mock HTTP | `API_tests/test_payments_api.py, API_tests/test_security.py` | `API_tests/test_payments_api.py:302` TestCallbacks.test_simulate_callback_nonexistent_transaction_returns_error |
| `POST /payments/callbacks/{gateway}` | yes | true no-mock HTTP | `API_tests/test_security.py` | `API_tests/test_security.py:71` TestCallbackSignatureVerification.test_callback_bad_signature_is_rejected |
| `POST /payments/compensation/trigger` | yes | true no-mock HTTP | `API_tests/test_coverage_gaps.py` | `API_tests/test_coverage_gaps.py:67` TestPaymentsCompensation.test_trigger_compensation_returns_202_with_sweeps |
| `POST /payments/imports` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:240` TestStatementImports.test_finance_can_upload_import |
| `POST /payments/imports/{id}/process` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:286` TestStatementImports.test_process_nonexistent_import_returns_404 |
| `POST /payments/refunds` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:148` TestCreateRefund.test_finance_can_create_refund |
| `POST /payments/refunds/{id}/approve` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:221` TestListRefunds.test_approve_nonexistent_refund_returns_404 |
| `POST /payments/refunds/{id}/process` | yes | true no-mock HTTP | `API_tests/test_payments_api.py` | `API_tests/test_payments_api.py:226` TestListRefunds.test_process_nonexistent_refund_returns_404 |
| `POST /payments/transactions` | yes | true no-mock HTTP | `API_tests/test_payments_api.py, API_tests/test_rbac_api.py` | `API_tests/test_payments_api.py:39` TestCreateTransaction.test_finance_can_create_transaction |
| `POST /reconciliation/runs` | yes | true no-mock HTTP | `API_tests/test_rbac_api.py, API_tests/test_reauth_gated.py` | `API_tests/test_rbac_api.py:269` TestReconciliationRbac.test_finance_can_start_run |
| `POST /reconciliation/statements` | yes | true no-mock HTTP | `API_tests/test_reconciliation_api.py` | `API_tests/test_reconciliation_api.py:33` TestReconciliationStatements.test_upload_statement_invalid_body_returns_400 |
| `POST /reporting/metrics` | yes | true no-mock HTTP | `API_tests/test_rbac_api.py, API_tests/test_reauth_gated.py` | `API_tests/test_rbac_api.py:309` TestReportingRbac.test_finance_can_create_metric |
| `POST /reporting/metrics/compute` | yes | true no-mock HTTP | `API_tests/test_reporting_api.py` | `API_tests/test_reporting_api.py:39` TestReportingMetrics.test_compute_metrics_invalid_body_returns_400 |
| `POST /reporting/schedules` | yes | true no-mock HTTP | `API_tests/test_reauth_gated.py` | `API_tests/test_reauth_gated.py:255` TestScheduleCreateReauth.test_create_schedule_without_reauth_returns_403 |
| `POST /reporting/schedules/{id}/trigger` | yes | true no-mock HTTP | `API_tests/test_reauth_gated.py` | `API_tests/test_reauth_gated.py:285` TestTriggerRunReauth.test_trigger_run_without_reauth_returns_403 |
| `PUT /notifications/channels/{channel}` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:343` TestChannelPreferences.test_upsert_email_channel_pref |
| `PUT /notifications/preferences` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py, API_tests/test_security.py` | `API_tests/test_notifications_api.py:141` TestPreferences.test_disable_dnd_succeeds |
| `PUT /notifications/rules/{id}` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:250` TestNotificationRules.test_update_nonexistent_rule_returns_404 |
| `PUT /notifications/subscriptions` | yes | true no-mock HTTP | `API_tests/test_notifications_api.py` | `API_tests/test_notifications_api.py:186` TestSubscriptions.test_update_subscriptions_with_empty_list |
| `PUT /ops/calendars/{id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:293` TestOpsCalendarsCrud.test_update_calendar_nonexistent_returns_404 |
| `PUT /ops/configs/{tid}/versions/{vid}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:325` TestOpsConfigVersions.test_update_version_nonexistent_returns_404 |
| `PUT /ops/routes/{id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:74` TestOpsRoutesCrud.test_update_route_persists_new_name |
| `PUT /ops/routes/{route_id}/stops/{stop_id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:209` TestOpsStopsCrud.test_update_stop_nonexistent_returns_404 |
| `PUT /ops/trips/{id}` | yes | true no-mock HTTP | `API_tests/test_ops_api.py` | `API_tests/test_ops_api.py:242` TestOpsTripsCrud.test_update_trip_nonexistent_returns_404 |
| `PUT /reporting/metrics/{id}` | yes | true no-mock HTTP | `API_tests/test_reauth_gated.py` | `API_tests/test_reauth_gated.py:227` TestMetricUpdateReauth.test_update_metric_without_reauth_returns_403 |
| `PUT /reporting/schedules/{id}` | yes | true no-mock HTTP | `API_tests/test_reporting_api.py` | `API_tests/test_reporting_api.py:68` TestReportingSchedules.test_update_schedule_nonexistent_returns_404 |

## API Test Classification
1. True No-Mock HTTP: `repo/API_tests/*.py` and `repo/e2e/tests/*.spec.ts`.
2. HTTP with Mocking: none detected.
3. Non-HTTP: `repo/unit_tests/*.py`, `repo/tests/*.rs`, `repo/frontend/tests/*.rs`.

## Mock Detection
- No `jest.mock`, `vi.mock`, `sinon.stub`, `monkeypatch`, or request interception in API/E2E test suites.
- API helper evidence: `repo/API_tests/conftest.py:310` uses `requests.request` against real `API_URL`.

## Coverage Summary
- total endpoints: **122**
- endpoints with HTTP tests: **122**
- endpoints with TRUE no-mock tests: **122**
- HTTP coverage %: **100.00%**
- True API coverage %: **100.00%**

## Unit Test Summary
### Backend Unit Tests
- Files: `repo/unit_tests/test_dnd_logic.py`, `repo/unit_tests/test_reconciliation_logic.py`, `repo/unit_tests/test_signature_logic.py`, `repo/unit_tests/test_alert_severity.py`, and Rust tests in `repo/tests/*.rs`.
- Covered modules: alerting detector, notifications bus/adapters, reconciliation discrepancy, payments signature, scheduler lock/idempotency.
- Important backend modules not unit-tested directly: auth middleware, audit handlers, reporting handlers.

### Frontend Unit Tests (STRICT REQUIREMENT)
- Frontend test files found: `repo/frontend/tests/component_states.spec.rs`, `repo/frontend/tests/api_service.spec.rs`, `repo/frontend/tests/dispatcher_workflows.spec.rs`, `repo/frontend/tests/inbox_panel.spec.rs`, `repo/frontend/tests/notification_logic.spec.rs`, `repo/frontend/tests/role_guard.spec.rs`, `repo/frontend/tests/service_contracts.spec.rs`.
- Framework/tools detected: `wasm_bindgen_test`.
- Components/modules covered: role/auth logic, notification reducer/store, service contracts, dispatcher conflict logic, ops page state logic.
- Important frontend components/modules not directly targeted: `repo/frontend/src/components/nav.rs`, `repo/frontend/src/components/notification_badge.rs`, `repo/frontend/src/components/notification_card.rs`, `repo/frontend/src/components/reauth_prompt.rs`.
- **Frontend unit tests: PRESENT**

### Cross-Layer Observation
- Both backend and frontend have testing; backend endpoint coverage breadth exceeds frontend component-level behavioral breadth.

## API Observability Check
- Mostly strong: tests generally show endpoint, request input, and response assertions.
- Weak spots: some tests are status-only checks with shallow response validation.

## Test Quality & Sufficiency
- Success/failure/validation/auth/RBAC/reauth/security scenarios are present.
- Gap: heavy use of non-existent-ID negative tests, fewer full stateful write-path lifecycle assertions.
- `run_tests.sh` is Docker-based (`repo/run_tests.sh:166`-`repo/run_tests.sh:209`) -> OK.

## End-to-End Expectations
- Fullstack E2E exists in `repo/e2e/tests/login.spec.ts`, `repo/e2e/tests/notifications.spec.ts`, `repo/e2e/tests/alerts.spec.ts`, `repo/e2e/tests/reporting.spec.ts`.
- E2E scope is meaningful but not exhaustive across all routes/features.

## Test Coverage Score (0–100)
- **88**

## Score Rationale
- High endpoint-level coverage and true no-mock API path evidence.
- Deduction for uneven assertion depth and partial E2E breadth.

## Key Gaps
- Limited deep state-transition assertions for some dispatcher/reporting write flows.
- Some frontend UI components lack direct tests.

## Confidence & Assumptions
- Confidence: high for static mapping.
- Assumption: dynamic `_path` constants in `test_reauth_gated.py` are exercised as written.
- Static inspection only; no runtime execution.

# README Audit

## High Priority Issues
- None.

## Medium Priority Issues
- Quick-start can be misread because frontend startup is profile-gated (`repo/README.md:24`, `repo/README.md:61`).

## Low Priority Issues
- Inconsistent CLI style (`docker-compose` vs `docker compose`) (`repo/README.md:12`, `repo/README.md:18`).

## Hard Gate Failures
- None.

## README Verdict (PASS / PARTIAL PASS / FAIL)
- **PASS**

## Final Verdicts
- Test Coverage Audit Verdict: **PASS with quality caveats**
- README Audit Verdict: **PASS**