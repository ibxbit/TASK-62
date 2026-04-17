"""
Alerting endpoint tests.

Endpoints covered
-----------------
  GET  /alerts                    list alerts (filters: status, severity, alert_type)
  GET  /alerts/stats              aggregated counts
  GET  /alerts/{id}               single alert
  POST /alerts/{id}/acknowledge   acknowledge an open alert
  POST /alerts/{id}/close         close an alert

Coverage
--------
  - Authenticated list returns 200 + array
  - Status and severity filters accepted
  - Stats endpoint returns expected shape
  - Non-existent alert ID → 404
  - Acknowledge and close non-existent → 404
  - Acknowledge without notes (optional field)
  - Acknowledge with notes
  - Unauthenticated requests → 401
  - staff_user (no AlertsRead) → 403
"""

import pytest

from conftest import TEST_USERS

NON_EXISTENT_ID = "00000000-0000-0000-0000-000000000000"


# ── List alerts ────────────────────────────────────────────────────────────────

class TestListAlerts:
    def test_admin_list_returns_200(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token)
        assert r.status_code == 200

    def test_list_returns_array(self, api, admin_token, test_user_ids):
        body = api("GET", "/alerts", token=admin_token).json()
        assert isinstance(body, list)

    def test_unauthenticated_returns_401(self, api):
        r = api("GET", "/alerts")
        assert r.status_code == 401

    def test_staff_cannot_list_alerts(self, api, staff_token, test_user_ids):
        """staff_user lacks AlertsRead."""
        r = api("GET", "/alerts", token=staff_token)
        assert r.status_code == 403

    def test_dispatcher_can_list_alerts(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/alerts", token=dispatcher_token)
        assert r.status_code == 200

    def test_finance_can_list_alerts(self, api, finance_token, test_user_ids):
        r = api("GET", "/alerts", token=finance_token)
        assert r.status_code == 200

    def test_filter_by_status_open(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token, params={"status": "open"})
        assert r.status_code == 200

    def test_filter_by_status_acknowledged(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token,
                params={"status": "acknowledged"})
        assert r.status_code == 200

    def test_filter_by_status_closed(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token, params={"status": "closed"})
        assert r.status_code == 200

    def test_filter_by_severity_warning(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token, params={"severity": "warning"})
        assert r.status_code == 200

    def test_filter_by_severity_critical(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token, params={"severity": "critical"})
        assert r.status_code == 200

    def test_filter_by_alert_type_kpi(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token,
                params={"alert_type": "kpi_anomaly"})
        assert r.status_code == 200

    def test_filter_by_alert_type_reconciliation(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token,
                params={"alert_type": "reconciliation_mismatch"})
        assert r.status_code == 200

    def test_invalid_status_filter_returns_error(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token,
                params={"status": "nonexistent_status"})
        assert r.status_code in (400, 422)

    def test_invalid_severity_filter_returns_error(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token,
                params={"severity": "super_critical"})
        assert r.status_code in (400, 422)

    def test_list_with_limit_param(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token, params={"limit": 10})
        assert r.status_code == 200

    def test_combined_filters(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token,
                params={"status": "open", "severity": "critical"})
        assert r.status_code == 200


# ── Alert stats ────────────────────────────────────────────────────────────────

class TestAlertStats:
    """The /alerts/stats response must have the aggregated-counts contract."""

    def test_stats_returns_full_contract(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts/stats", token=admin_token)
        assert r.status_code == 200
        body = r.json()
        assert isinstance(body, dict)
        # Contract — these keys are always present.
        for key in ("by_severity", "by_status", "open_total"):
            assert key in body, f"missing key {key!r} in /alerts/stats body: {body!r}"
        assert isinstance(body["by_severity"], dict)
        assert isinstance(body["by_status"], dict)
        assert isinstance(body["open_total"], int)
        assert body["open_total"] >= 0

    def test_stats_unauthenticated_returns_401(self, api):
        r = api("GET", "/alerts/stats")
        assert r.status_code == 401
        assert r.json().get("code") == "UNAUTHORIZED"

    def test_staff_cannot_access_stats(self, api, staff_token, test_user_ids):
        r = api("GET", "/alerts/stats", token=staff_token)
        assert r.status_code == 403
        assert r.json().get("code") == "FORBIDDEN"

    def test_finance_can_access_stats(self, api, finance_token, test_user_ids):
        r = api("GET", "/alerts/stats", token=finance_token)
        assert r.status_code == 200


# ── Get single alert ──────────────────────────────────────────────────────────

class TestGetAlert:
    def test_nonexistent_alert_returns_404(self, api, admin_token, test_user_ids):
        r = api("GET", f"/alerts/{NON_EXISTENT_ID}", token=admin_token)
        assert r.status_code == 404
        body = r.json()
        assert body.get("code") == "NOT_FOUND"
        assert "Alert" in body.get("error", "") or "not found" in body.get("error", "").lower()

    def test_unauthenticated_returns_401(self, api):
        r = api("GET", f"/alerts/{NON_EXISTENT_ID}")
        assert r.status_code == 401
        assert r.json().get("code") == "UNAUTHORIZED"

    def test_staff_cannot_get_alert(self, api, staff_token, test_user_ids):
        r = api("GET", f"/alerts/{NON_EXISTENT_ID}", token=staff_token)
        assert r.status_code == 403
        assert r.json().get("code") == "FORBIDDEN"


# ── Acknowledge alert ─────────────────────────────────────────────────────────

class TestAcknowledgeAlert:
    def test_acknowledge_nonexistent_returns_404(self, api, admin_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=admin_token, json={})
        assert r.status_code == 404

    def test_acknowledge_with_notes_nonexistent_returns_404(self, api, admin_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=admin_token,
                json={"notes": "Investigating the KPI spike on route 12."})
        assert r.status_code == 404

    def test_unauthenticated_returns_401(self, api):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge", json={})
        assert r.status_code == 401

    def test_staff_cannot_acknowledge(self, api, staff_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=staff_token, json={})
        assert r.status_code == 403

    def test_dispatcher_cannot_acknowledge(self, api, dispatcher_token, test_user_ids):
        """Dispatcher has AlertsRead but NOT AlertsManage."""
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=dispatcher_token, json={})
        assert r.status_code == 403

    def test_admin_acknowledge_nonexistent_returns_404(self, api, admin_token, test_user_ids):
        """Admin has AlertsManage; non-existent alert → 404 NOT_FOUND, not 403."""
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=admin_token, json={})
        assert r.status_code == 404
        assert r.json().get("code") == "NOT_FOUND"

    def test_finance_acknowledge_nonexistent_returns_404(self, api, finance_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=finance_token, json={})
        assert r.status_code == 404
        assert r.json().get("code") == "NOT_FOUND"


# ── Close alert ────────────────────────────────────────────────────────────────

class TestCloseAlert:
    def test_close_nonexistent_returns_404(self, api, admin_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/close",
                token=admin_token, json={})
        assert r.status_code == 404

    def test_close_with_reason_nonexistent_returns_404(self, api, admin_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/close",
                token=admin_token,
                json={"reason": "False positive — reconciliation was rerun successfully."})
        assert r.status_code == 404

    def test_unauthenticated_returns_401(self, api):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/close", json={})
        assert r.status_code == 401

    def test_staff_cannot_close(self, api, staff_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/close",
                token=staff_token, json={})
        assert r.status_code == 403

    def test_dispatcher_cannot_close(self, api, dispatcher_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/close",
                token=dispatcher_token, json={})
        assert r.status_code == 403

    def test_admin_close_nonexistent_returns_404(self, api, admin_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/close",
                token=admin_token, json={})
        assert r.status_code == 404
        assert r.json().get("code") == "NOT_FOUND"

    def test_finance_close_nonexistent_returns_404(self, api, finance_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/close",
                token=finance_token, json={})
        assert r.status_code == 404
        assert r.json().get("code") == "NOT_FOUND"


# ── Alert lifecycle (if alerts exist) ────────────────────────────────────────

class TestAlertLifecycle:
    """
    If the running system has any open alerts, exercise the full
    open → acknowledged → closed transition.
    """

    def test_full_lifecycle_if_open_alert_exists(self, api, admin_token, test_user_ids):
        alerts_r = api("GET", "/alerts", token=admin_token,
                       params={"status": "open", "limit": 1})
        if alerts_r.status_code != 200:
            pytest.skip("Cannot list alerts")
        alerts = alerts_r.json()
        if not alerts:
            pytest.skip("No open alerts in the system")

        alert_id = alerts[0]["id"]

        ack_r = api("POST", f"/alerts/{alert_id}/acknowledge",
                    token=admin_token, json={"notes": "Lifecycle test acknowledge."})
        assert ack_r.status_code == 200

        # Verify status changed
        get_r = api("GET", f"/alerts/{alert_id}", token=admin_token)
        assert get_r.status_code == 200
        assert get_r.json()["status"] == "acknowledged"

        close_r = api("POST", f"/alerts/{alert_id}/close",
                      token=admin_token,
                      json={"reason": "Lifecycle test close."})
        assert close_r.status_code == 200

        # Verify terminal state
        final_r = api("GET", f"/alerts/{alert_id}", token=admin_token)
        assert final_r.json()["status"] == "closed"

    def test_closed_alert_cannot_be_acknowledged(self, api, admin_token, test_user_ids):
        """Closed is a terminal state — re-acknowledging should fail."""
        alerts_r = api("GET", "/alerts", token=admin_token,
                       params={"status": "closed", "limit": 1})
        if alerts_r.status_code != 200:
            pytest.skip("Cannot list alerts")
        closed_alerts = alerts_r.json()
        if not closed_alerts:
            pytest.skip("No closed alerts in the system")

        alert_id = closed_alerts[0]["id"]
        r = api("POST", f"/alerts/{alert_id}/acknowledge",
                token=admin_token, json={})
        # Expect 409 (conflict) or 400 (invalid transition), not 200
        assert r.status_code in (400, 409, 422)
