"""
RBAC enforcement tests — verifies that each role can only reach the
endpoints it is explicitly permitted to access.

Role permission summary
-----------------------
  operations_admin  — full ops, full notifications (incl. announce), audit read,
                      payments read + refund approve, full reporting, alerts manage
  dispatcher        — ops read/write (no delete, no config publish), own notifications,
                      reporting read, alerts read; NO payments, NO audit
  finance_analyst   — payments full, ops read-only, own notifications,
                      reporting full, alerts manage; NO audit, NO ops write/delete
  staff_user        — ops read-only, own inbox + DND, reporting read;
                      NO payments, NO audit, NO ops write

Test strategy
-------------
  For denied endpoints: assert 403 Forbidden
  For allowed endpoints with no matching resource: assert NOT 403
    (accept 200, 201, 400, 404, 422 — any response except 403 proves the gate opened)
"""

import uuid

import pytest

from conftest import TEST_USERS

NON_EXISTENT_ID = "00000000-0000-0000-0000-000000000000"


def _allowed(status_code: int) -> bool:
    """True if the response indicates the permission check passed."""
    return status_code != 403


def _denied(status_code: int) -> bool:
    return status_code == 403


# ── Audit log access ──────────────────────────────────────────────────────────

class TestAuditRbac:
    def test_admin_can_read_audit_logs(self, api, admin_token, test_user_ids):
        r = api("GET", "/audit/logs", token=admin_token)
        assert _allowed(r.status_code), f"Admin should access audit logs, got {r.status_code}"

    def test_dispatcher_cannot_read_audit_logs(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/audit/logs", token=dispatcher_token)
        assert _denied(r.status_code)

    def test_finance_cannot_read_audit_logs(self, api, finance_token, test_user_ids):
        r = api("GET", "/audit/logs", token=finance_token)
        assert _denied(r.status_code)

    def test_staff_cannot_read_audit_logs(self, api, staff_token, test_user_ids):
        r = api("GET", "/audit/logs", token=staff_token)
        assert _denied(r.status_code)

    def test_unauthenticated_cannot_read_audit_logs(self, api):
        r = api("GET", "/audit/logs")
        assert r.status_code == 401


# ── Payments write access ─────────────────────────────────────────────────────

class TestPaymentsWriteRbac:
    _TXN_BODY = {
        "idempotency_key": f"rbac_test_{uuid.uuid4().hex[:8]}",
        "amount": "1.00",
        "payment_method": "cash",
    }

    def test_finance_can_create_transactions(self, api, finance_token, test_user_ids):
        body = {**self._TXN_BODY, "idempotency_key": f"rbac_fin_{uuid.uuid4().hex[:8]}"}
        r = api("POST", "/payments/transactions", token=finance_token, json=body)
        assert _allowed(r.status_code)

    def test_admin_cannot_create_transactions(self, api, admin_token, test_user_ids):
        """operations_admin has PaymentsTransactionsRead but NOT PaymentsTransactionsWrite."""
        body = {**self._TXN_BODY, "idempotency_key": f"rbac_adm_{uuid.uuid4().hex[:8]}"}
        r = api("POST", "/payments/transactions", token=admin_token, json=body)
        assert _denied(r.status_code)

    def test_dispatcher_cannot_create_transactions(self, api, dispatcher_token, test_user_ids):
        body = {**self._TXN_BODY, "idempotency_key": f"rbac_dis_{uuid.uuid4().hex[:8]}"}
        r = api("POST", "/payments/transactions", token=dispatcher_token, json=body)
        assert _denied(r.status_code)

    def test_staff_cannot_create_transactions(self, api, staff_token, test_user_ids):
        body = {**self._TXN_BODY, "idempotency_key": f"rbac_stf_{uuid.uuid4().hex[:8]}"}
        r = api("POST", "/payments/transactions", token=staff_token, json=body)
        assert _denied(r.status_code)

    def test_finance_can_read_transactions(self, api, finance_token, test_user_ids):
        r = api("GET", "/payments/transactions", token=finance_token)
        assert _allowed(r.status_code)

    def test_admin_can_read_transactions(self, api, admin_token, test_user_ids):
        r = api("GET", "/payments/transactions", token=admin_token)
        assert _allowed(r.status_code)

    def test_dispatcher_cannot_read_transactions(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/payments/transactions", token=dispatcher_token)
        assert _denied(r.status_code)

    def test_staff_cannot_read_transactions(self, api, staff_token, test_user_ids):
        r = api("GET", "/payments/transactions", token=staff_token)
        assert _denied(r.status_code)


# ── Ops route mutation ────────────────────────────────────────────────────────

class TestOpsWriteRbac:
    _ROUTE_BODY = {
        "route_code": f"RBAC_{uuid.uuid4().hex[:6].upper()}",
        "name": "RBAC Test Route",
        "description": "Created by RBAC test",
    }

    def test_admin_can_create_routes(self, api, admin_token, test_user_ids):
        body = {**self._ROUTE_BODY, "route_code": f"ADM_{uuid.uuid4().hex[:6].upper()}"}
        r = api("POST", "/ops/routes", token=admin_token, json=body)
        assert _allowed(r.status_code)

    def test_dispatcher_can_create_routes(self, api, dispatcher_token, test_user_ids):
        """Dispatchers have OpsRoutesWrite."""
        body = {**self._ROUTE_BODY, "route_code": f"DIS_{uuid.uuid4().hex[:6].upper()}"}
        r = api("POST", "/ops/routes", token=dispatcher_token, json=body)
        assert _allowed(r.status_code)

    def test_finance_cannot_create_routes(self, api, finance_token, test_user_ids):
        """Finance has OpsRoutesRead only — not Write."""
        body = {**self._ROUTE_BODY, "route_code": f"FIN_{uuid.uuid4().hex[:6].upper()}"}
        r = api("POST", "/ops/routes", token=finance_token, json=body)
        assert _denied(r.status_code)

    def test_staff_cannot_create_routes(self, api, staff_token, test_user_ids):
        body = {**self._ROUTE_BODY, "route_code": f"STF_{uuid.uuid4().hex[:6].upper()}"}
        r = api("POST", "/ops/routes", token=staff_token, json=body)
        assert _denied(r.status_code)

    def test_admin_can_delete_routes(self, api, admin_token, test_user_ids):
        r = api("DELETE", f"/ops/routes/{NON_EXISTENT_ID}", token=admin_token)
        assert _allowed(r.status_code)

    def test_dispatcher_cannot_delete_routes(self, api, dispatcher_token, test_user_ids):
        """Dispatchers lack OpsRoutesDelete."""
        r = api("DELETE", f"/ops/routes/{NON_EXISTENT_ID}", token=dispatcher_token)
        assert _denied(r.status_code)

    def test_finance_cannot_delete_routes(self, api, finance_token, test_user_ids):
        r = api("DELETE", f"/ops/routes/{NON_EXISTENT_ID}", token=finance_token)
        assert _denied(r.status_code)

    def test_staff_cannot_delete_routes(self, api, staff_token, test_user_ids):
        r = api("DELETE", f"/ops/routes/{NON_EXISTENT_ID}", token=staff_token)
        assert _denied(r.status_code)

    def test_all_roles_can_read_routes(self, api, admin_token, dispatcher_token,
                                        finance_token, staff_token, test_user_ids):
        for token, label in [
            (admin_token, "admin"),
            (dispatcher_token, "dispatcher"),
            (finance_token, "finance"),
            (staff_token, "staff"),
        ]:
            r = api("GET", "/ops/routes", token=token)
            assert _allowed(r.status_code), f"{label} should be able to read routes"


# ── Ops config publish (admin-only among seeded roles) ────────────────────────

class TestOpsConfigRbac:
    def test_dispatcher_cannot_publish_config_version(self, api, dispatcher_token, test_user_ids):
        """Dispatcher has OpsConfigRead but NOT OpsConfigPublish."""
        r = api("POST",
                f"/ops/configs/{NON_EXISTENT_ID}/versions/{NON_EXISTENT_ID}/publish",
                token=dispatcher_token)
        assert _denied(r.status_code)

    def test_admin_can_attempt_config_publish(self, api, admin_token, test_user_ids):
        r = api("POST",
                f"/ops/configs/{NON_EXISTENT_ID}/versions/{NON_EXISTENT_ID}/publish",
                token=admin_token)
        assert _allowed(r.status_code)


# ── Announcements (operations_admin only) ─────────────────────────────────────

class TestAnnouncementRbac:
    _ANN = {"title": "RBAC Test", "message": "RBAC announcement test."}

    def test_admin_can_announce(self, api, admin_token, test_user_ids):
        r = api("POST", "/notifications/announce", token=admin_token, json=self._ANN)
        assert _allowed(r.status_code)

    def test_dispatcher_cannot_announce(self, api, dispatcher_token, test_user_ids):
        r = api("POST", "/notifications/announce", token=dispatcher_token, json=self._ANN)
        assert _denied(r.status_code)

    def test_finance_cannot_announce(self, api, finance_token, test_user_ids):
        r = api("POST", "/notifications/announce", token=finance_token, json=self._ANN)
        assert _denied(r.status_code)

    def test_staff_cannot_announce(self, api, staff_token, test_user_ids):
        r = api("POST", "/notifications/announce", token=staff_token, json=self._ANN)
        assert _denied(r.status_code)


# ── Alerts management ─────────────────────────────────────────────────────────

class TestAlertsRbac:
    def test_admin_can_read_alerts(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token)
        assert _allowed(r.status_code)

    def test_finance_can_read_alerts(self, api, finance_token, test_user_ids):
        r = api("GET", "/alerts", token=finance_token)
        assert _allowed(r.status_code)

    def test_dispatcher_can_read_alerts(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/alerts", token=dispatcher_token)
        assert _allowed(r.status_code)

    def test_staff_cannot_read_alerts(self, api, staff_token, test_user_ids):
        """staff_user lacks AlertsRead."""
        r = api("GET", "/alerts", token=staff_token)
        assert _denied(r.status_code)

    def test_admin_can_manage_alerts(self, api, admin_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=admin_token, json={})
        assert _allowed(r.status_code)

    def test_dispatcher_cannot_manage_alerts(self, api, dispatcher_token, test_user_ids):
        """Dispatcher has AlertsRead but NOT AlertsManage."""
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=dispatcher_token, json={})
        assert _denied(r.status_code)

    def test_finance_can_manage_alerts(self, api, finance_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=finance_token, json={})
        assert _allowed(r.status_code)


# ── Reconciliation access (finance + admin read, finance runs) ────────────────

class TestReconciliationRbac:
    def test_finance_can_list_runs(self, api, finance_token, test_user_ids):
        r = api("GET", "/reconciliation/runs", token=finance_token)
        assert _allowed(r.status_code)

    def test_admin_can_list_runs(self, api, admin_token, test_user_ids):
        r = api("GET", "/reconciliation/runs", token=admin_token)
        assert _allowed(r.status_code)

    def test_dispatcher_cannot_list_runs(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/reconciliation/runs", token=dispatcher_token)
        assert _denied(r.status_code)

    def test_staff_cannot_list_runs(self, api, staff_token, test_user_ids):
        r = api("GET", "/reconciliation/runs", token=staff_token)
        assert _denied(r.status_code)

    def test_finance_can_start_run(self, api, finance_token, test_user_ids):
        r = api("POST", "/reconciliation/runs", token=finance_token,
                json={"statement_id": NON_EXISTENT_ID})
        assert _allowed(r.status_code)

    def test_admin_cannot_start_run(self, api, admin_token, test_user_ids):
        """operations_admin has PaymentsReconciliationRead but NOT PaymentsReconciliationRun."""
        r = api("POST", "/reconciliation/runs", token=admin_token,
                json={"statement_id": NON_EXISTENT_ID})
        assert _denied(r.status_code)


# ── Reporting access ──────────────────────────────────────────────────────────

class TestReportingRbac:
    def test_admin_can_read_reporting(self, api, admin_token, test_user_ids):
        r = api("GET", "/reporting/metrics", token=admin_token)
        assert _allowed(r.status_code)

    def test_finance_can_read_reporting(self, api, finance_token, test_user_ids):
        r = api("GET", "/reporting/metrics", token=finance_token)
        assert _allowed(r.status_code)

    def test_dispatcher_can_read_reporting(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/reporting/metrics", token=dispatcher_token)
        assert _allowed(r.status_code)

    def test_staff_can_read_reporting(self, api, staff_token, test_user_ids):
        r = api("GET", "/reporting/metrics", token=staff_token)
        assert _allowed(r.status_code)

    def test_staff_cannot_create_metric(self, api, staff_token, test_user_ids):
        """staff_user has ReportingRead only — not ReportingMetricsManage."""
        r = api("POST", "/reporting/metrics", token=staff_token,
                json={"name": "test_metric", "query": "SELECT 1"})
        assert _denied(r.status_code)

    def test_dispatcher_cannot_create_metric(self, api, dispatcher_token, test_user_ids):
        r = api("POST", "/reporting/metrics", token=dispatcher_token,
                json={"name": "test_metric", "query": "SELECT 1"})
        assert _denied(r.status_code)

    def test_finance_can_create_metric(self, api, finance_token, test_user_ids):
        r = api("POST", "/reporting/metrics", token=finance_token,
                json={"name": "test_metric", "query": "SELECT 1"})
        assert _allowed(r.status_code)
