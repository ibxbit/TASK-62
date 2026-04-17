"""
RBAC enforcement tests — exact status codes and response-body contracts for
every role × endpoint combination.

Conftest reauthenticates each role token after login, so reauth-gated
endpoints return the RBAC-decision status (200/201/404/etc.) rather than the
reauth gate's 403.

Role permission summary (from db/seeds/002_rbac_seed.sql):
  operations_admin  — full ops, full notifications (incl. announce), audit read,
                      payments read only (no write), full reporting, alerts manage
  dispatcher        — ops read/write (no delete, no config publish), own notifications,
                      reporting read, alerts read; NO payments, NO audit
  finance_analyst   — payments full, ops read-only, own notifications,
                      reporting full, alerts manage; NO audit, NO ops write/delete
  staff_user        — ops read-only, own inbox + DND, reporting read;
                      NO payments, NO audit, NO ops write, NO alerts read
"""

import uuid
import pytest

from conftest import TEST_USERS

NON_EXISTENT_ID = "00000000-0000-0000-0000-000000000000"


def _assert_forbidden(resp):
    assert resp.status_code == 403, (
        f"expected 403, got {resp.status_code}: {resp.text[:200]}"
    )
    body = resp.json()
    assert body.get("code") == "FORBIDDEN", body


def _assert_allowed(resp, *, expected: tuple[int, ...]):
    """Permission gate passed — response must be one of the expected success/not-found codes."""
    assert resp.status_code in expected, (
        f"expected one of {expected}, got {resp.status_code}: {resp.text[:200]}"
    )


# ── Audit log access ──────────────────────────────────────────────────────────

class TestAuditRbac:
    def test_admin_can_read_audit_logs(self, api, admin_token, test_user_ids):
        r = api("GET", "/audit/logs", token=admin_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)

    def test_dispatcher_cannot_read_audit_logs(self, api, dispatcher_token, test_user_ids):
        _assert_forbidden(api("GET", "/audit/logs", token=dispatcher_token))

    def test_finance_cannot_read_audit_logs(self, api, finance_token, test_user_ids):
        _assert_forbidden(api("GET", "/audit/logs", token=finance_token))

    def test_staff_cannot_read_audit_logs(self, api, staff_token, test_user_ids):
        _assert_forbidden(api("GET", "/audit/logs", token=staff_token))

    def test_unauthenticated_cannot_read_audit_logs(self, api):
        r = api("GET", "/audit/logs")
        assert r.status_code == 401
        assert r.json().get("code") == "UNAUTHORIZED"


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
        # Finance has the write permission; allow 201 (created) or 200 (idempotent).
        _assert_allowed(r, expected=(200, 201))
        assert "id" in r.json()

    def test_admin_cannot_create_transactions(self, api, admin_token, test_user_ids):
        """operations_admin has PaymentsTransactionsRead but NOT Write."""
        body = {**self._TXN_BODY, "idempotency_key": f"rbac_adm_{uuid.uuid4().hex[:8]}"}
        _assert_forbidden(api("POST", "/payments/transactions", token=admin_token, json=body))

    def test_dispatcher_cannot_create_transactions(self, api, dispatcher_token, test_user_ids):
        body = {**self._TXN_BODY, "idempotency_key": f"rbac_dis_{uuid.uuid4().hex[:8]}"}
        _assert_forbidden(api("POST", "/payments/transactions", token=dispatcher_token, json=body))

    def test_staff_cannot_create_transactions(self, api, staff_token, test_user_ids):
        body = {**self._TXN_BODY, "idempotency_key": f"rbac_stf_{uuid.uuid4().hex[:8]}"}
        _assert_forbidden(api("POST", "/payments/transactions", token=staff_token, json=body))

    def test_finance_can_read_transactions(self, api, finance_token, test_user_ids):
        r = api("GET", "/payments/transactions", token=finance_token)
        _assert_allowed(r, expected=(200,))

    def test_admin_can_read_transactions(self, api, admin_token, test_user_ids):
        r = api("GET", "/payments/transactions", token=admin_token)
        _assert_allowed(r, expected=(200,))

    def test_dispatcher_cannot_read_transactions(self, api, dispatcher_token, test_user_ids):
        _assert_forbidden(api("GET", "/payments/transactions", token=dispatcher_token))

    def test_staff_cannot_read_transactions(self, api, staff_token, test_user_ids):
        _assert_forbidden(api("GET", "/payments/transactions", token=staff_token))


# ── Ops route mutation ────────────────────────────────────────────────────────

class TestOpsWriteRbac:
    def test_admin_can_create_routes(self, api, admin_token, test_user_ids):
        code = f"ADM_{uuid.uuid4().hex[:6].upper()}"
        r = api("POST", "/ops/routes", token=admin_token,
                json={"code": code, "name": "Admin route", "description": "x"})
        assert r.status_code == 201, r.text
        assert r.json()["code"] == code

    def test_dispatcher_can_create_routes(self, api, dispatcher_token, test_user_ids):
        code = f"DIS_{uuid.uuid4().hex[:6].upper()}"
        r = api("POST", "/ops/routes", token=dispatcher_token,
                json={"code": code, "name": "Dispatch route", "description": "x"})
        assert r.status_code == 201, r.text

    def test_finance_cannot_create_routes(self, api, finance_token, test_user_ids):
        code = f"FIN_{uuid.uuid4().hex[:6].upper()}"
        r = api("POST", "/ops/routes", token=finance_token,
                json={"code": code, "name": "Denied", "description": "x"})
        _assert_forbidden(r)

    def test_staff_cannot_create_routes(self, api, staff_token, test_user_ids):
        code = f"STF_{uuid.uuid4().hex[:6].upper()}"
        r = api("POST", "/ops/routes", token=staff_token,
                json={"code": code, "name": "Denied", "description": "x"})
        _assert_forbidden(r)

    def test_admin_can_delete_routes(self, api, admin_token, test_user_ids):
        r = api("DELETE", f"/ops/routes/{NON_EXISTENT_ID}", token=admin_token)
        # Admin has delete permission — non-existent resource → 404, not 403.
        assert r.status_code == 404
        assert r.json().get("code") == "NOT_FOUND"

    def test_dispatcher_cannot_delete_routes(self, api, dispatcher_token, test_user_ids):
        _assert_forbidden(api("DELETE", f"/ops/routes/{NON_EXISTENT_ID}", token=dispatcher_token))

    def test_finance_cannot_delete_routes(self, api, finance_token, test_user_ids):
        _assert_forbidden(api("DELETE", f"/ops/routes/{NON_EXISTENT_ID}", token=finance_token))

    def test_staff_cannot_delete_routes(self, api, staff_token, test_user_ids):
        _assert_forbidden(api("DELETE", f"/ops/routes/{NON_EXISTENT_ID}", token=staff_token))

    def test_all_roles_can_read_routes(self, api, admin_token, dispatcher_token,
                                       finance_token, staff_token, test_user_ids):
        for label, token in [
            ("admin", admin_token),
            ("dispatcher", dispatcher_token),
            ("finance", finance_token),
            ("staff", staff_token),
        ]:
            r = api("GET", "/ops/routes", token=token)
            assert r.status_code == 200, f"{label} GET /ops/routes: {r.status_code}"


# ── Ops config publish (admin-only; reauth-gated) ────────────────────────────

class TestOpsConfigRbac:
    def test_dispatcher_cannot_publish_config_version(self, api, dispatcher_token, test_user_ids):
        _assert_forbidden(api(
            "POST",
            f"/ops/configs/{NON_EXISTENT_ID}/versions/{NON_EXISTENT_ID}/publish",
            token=dispatcher_token,
        ))

    def test_finance_cannot_publish_config_version(self, api, finance_token, test_user_ids):
        _assert_forbidden(api(
            "POST",
            f"/ops/configs/{NON_EXISTENT_ID}/versions/{NON_EXISTENT_ID}/publish",
            token=finance_token,
        ))

    def test_staff_cannot_publish_config_version(self, api, staff_token, test_user_ids):
        _assert_forbidden(api(
            "POST",
            f"/ops/configs/{NON_EXISTENT_ID}/versions/{NON_EXISTENT_ID}/publish",
            token=staff_token,
        ))


# ── Announcements (operations_admin only) ─────────────────────────────────────

class TestAnnouncementRbac:
    _ANN = {"title": "RBAC Test", "message": "RBAC announcement test."}

    def test_admin_can_announce(self, api, admin_token, test_user_ids):
        r = api("POST", "/notifications/announce", token=admin_token, json=self._ANN)
        _assert_allowed(r, expected=(200, 201))

    def test_dispatcher_cannot_announce(self, api, dispatcher_token, test_user_ids):
        _assert_forbidden(api("POST", "/notifications/announce", token=dispatcher_token, json=self._ANN))

    def test_finance_cannot_announce(self, api, finance_token, test_user_ids):
        _assert_forbidden(api("POST", "/notifications/announce", token=finance_token, json=self._ANN))

    def test_staff_cannot_announce(self, api, staff_token, test_user_ids):
        _assert_forbidden(api("POST", "/notifications/announce", token=staff_token, json=self._ANN))


# ── Alerts management ─────────────────────────────────────────────────────────

class TestAlertsRbac:
    def test_admin_can_read_alerts(self, api, admin_token, test_user_ids):
        r = api("GET", "/alerts", token=admin_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)

    def test_finance_can_read_alerts(self, api, finance_token, test_user_ids):
        r = api("GET", "/alerts", token=finance_token)
        assert r.status_code == 200

    def test_dispatcher_can_read_alerts(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/alerts", token=dispatcher_token)
        assert r.status_code == 200

    def test_staff_cannot_read_alerts(self, api, staff_token, test_user_ids):
        _assert_forbidden(api("GET", "/alerts", token=staff_token))

    def test_admin_can_manage_alerts(self, api, admin_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=admin_token, json={})
        # Permission OK → non-existent alert → 404.
        assert r.status_code == 404
        assert r.json().get("code") == "NOT_FOUND"

    def test_dispatcher_cannot_manage_alerts(self, api, dispatcher_token, test_user_ids):
        _assert_forbidden(api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                              token=dispatcher_token, json={}))

    def test_finance_can_manage_alerts(self, api, finance_token, test_user_ids):
        r = api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                token=finance_token, json={})
        assert r.status_code == 404

    def test_staff_cannot_manage_alerts(self, api, staff_token, test_user_ids):
        _assert_forbidden(api("POST", f"/alerts/{NON_EXISTENT_ID}/acknowledge",
                              token=staff_token, json={}))


# ── Reconciliation access ─────────────────────────────────────────────────────

class TestReconciliationRbac:
    def test_finance_can_list_runs(self, api, finance_token, test_user_ids):
        r = api("GET", "/reconciliation/runs", token=finance_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)

    def test_admin_can_list_runs(self, api, admin_token, test_user_ids):
        r = api("GET", "/reconciliation/runs", token=admin_token)
        assert r.status_code == 200

    def test_dispatcher_cannot_list_runs(self, api, dispatcher_token, test_user_ids):
        _assert_forbidden(api("GET", "/reconciliation/runs", token=dispatcher_token))

    def test_staff_cannot_list_runs(self, api, staff_token, test_user_ids):
        _assert_forbidden(api("GET", "/reconciliation/runs", token=staff_token))

    def test_finance_can_start_run(self, api, finance_token, test_user_ids):
        """Finance has the run permission; a non-existent statement yields 404/400 after RBAC."""
        r = api("POST", "/reconciliation/runs", token=finance_token,
                json={"statement_id": NON_EXISTENT_ID})
        _assert_allowed(r, expected=(200, 201, 400, 404))

    def test_admin_cannot_start_run(self, api, admin_token, test_user_ids):
        """operations_admin has ReconciliationRead but NOT Run."""
        _assert_forbidden(api("POST", "/reconciliation/runs", token=admin_token,
                              json={"statement_id": NON_EXISTENT_ID}))


# ── Reporting access ──────────────────────────────────────────────────────────

class TestReportingRbac:
    _METRIC_BODY = {"name": "rbac_test_metric", "query": "SELECT 1"}

    def test_admin_can_read_reporting(self, api, admin_token, test_user_ids):
        r = api("GET", "/reporting/metrics", token=admin_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)

    def test_finance_can_read_reporting(self, api, finance_token, test_user_ids):
        r = api("GET", "/reporting/metrics", token=finance_token)
        assert r.status_code == 200

    def test_dispatcher_can_read_reporting(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/reporting/metrics", token=dispatcher_token)
        assert r.status_code == 200

    def test_staff_can_read_reporting(self, api, staff_token, test_user_ids):
        r = api("GET", "/reporting/metrics", token=staff_token)
        assert r.status_code == 200

    def test_staff_cannot_create_metric(self, api, staff_token, test_user_ids):
        _assert_forbidden(api("POST", "/reporting/metrics", token=staff_token, json=self._METRIC_BODY))

    def test_dispatcher_cannot_create_metric(self, api, dispatcher_token, test_user_ids):
        _assert_forbidden(api("POST", "/reporting/metrics", token=dispatcher_token, json=self._METRIC_BODY))

    def test_finance_can_create_metric(self, api, finance_token, test_user_ids):
        """Finance has reporting:metrics:manage — reauth-gated; fixture reauthed the token."""
        r = api("POST", "/reporting/metrics", token=finance_token, json=self._METRIC_BODY)
        _assert_allowed(r, expected=(200, 201, 400, 422))
