"""
Tests for the endpoints that were previously uncovered in the initial audit.

Strict contract assertions for:
  * POST /notifications/receipt     — delivery-receipt ack with real shape
  * GET  /payments/compensation/jobs — list job history
  * POST /payments/compensation/trigger — starts sweeps, 202 Accepted
  * GET  /audit/logs/:id            — single audit entry read
"""

import pytest

from conftest import TEST_USERS

NON_EXISTENT_ID = "00000000-0000-0000-0000-000000000000"


def _assert_error_body(resp, expected_code: str, status: int):
    assert resp.status_code == status, f"expected {status}, got {resp.status_code}: {resp.text}"
    body = resp.json()
    assert isinstance(body, dict)
    assert body.get("code") == expected_code, f"got {body!r}"


# ── Notification receipt ─────────────────────────────────────────────────────

class TestNotificationReceipt:
    """POST /notifications/receipt — {delivery_ids: [...]} → {promoted: N}."""

    def test_receipt_valid_body_returns_promoted_count(self, api, admin_token, test_user_ids):
        r = api("POST", "/notifications/receipt", token=admin_token,
                json={"delivery_ids": [NON_EXISTENT_ID]})
        assert r.status_code == 200
        body = r.json()
        assert "promoted" in body and isinstance(body["promoted"], int)
        # No deliveries exist with this id, so promoted must be 0.
        assert body["promoted"] == 0

    def test_receipt_missing_field_returns_400(self, api, admin_token, test_user_ids):
        r = api("POST", "/notifications/receipt", token=admin_token,
                json={"notification_id": NON_EXISTENT_ID, "channel": "email"})
        assert r.status_code == 400
        assert "delivery_ids" in r.text or "field" in r.text.lower()

    def test_receipt_unauthenticated_returns_401(self, api):
        r = api("POST", "/notifications/receipt",
                json={"delivery_ids": [NON_EXISTENT_ID]})
        _assert_error_body(r, "UNAUTHORIZED", 401)


# ── Payments compensation ────────────────────────────────────────────────────

class TestPaymentsCompensation:
    """Compensation sweeps — list jobs + trigger."""

    def test_list_compensation_jobs_returns_history(self, api, finance_token, test_user_ids):
        r = api("GET", "/payments/compensation/jobs", token=finance_token)
        assert r.status_code == 200
        body = r.json()
        assert isinstance(body, list)
        # The background scheduler has already run at least one sweep cycle.
        for job in body:
            for key in ("id", "job_type", "status", "affected_count", "started_at"):
                assert key in job, f"job missing {key}: {job}"

    def test_trigger_compensation_returns_202_with_sweeps(self, api, finance_token, test_user_ids):
        r = api("POST", "/payments/compensation/trigger", token=finance_token,
                json={})
        assert r.status_code == 202
        body = r.json()
        assert "sweeps" in body and isinstance(body["sweeps"], list)
        # Must include the three scheduled sweep types.
        for expected in ("stuck_transactions", "pending_refunds", "callback_retry"):
            assert expected in body["sweeps"]
        assert "message" in body

    def test_unauthenticated_compensation_jobs_returns_401(self, api):
        r = api("GET", "/payments/compensation/jobs")
        _assert_error_body(r, "UNAUTHORIZED", 401)

    def test_dispatcher_cannot_list_compensation_jobs(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/payments/compensation/jobs", token=dispatcher_token)
        _assert_error_body(r, "FORBIDDEN", 403)

    def test_staff_cannot_trigger_compensation(self, api, staff_token, test_user_ids):
        r = api("POST", "/payments/compensation/trigger", token=staff_token,
                json={})
        _assert_error_body(r, "FORBIDDEN", 403)


# ── Audit log detail ────────────────────────────────────────────────────────

class TestAuditLogDetail:
    """GET /audit/logs/:id — admin-only."""

    def test_get_audit_log_nonexistent_returns_404(self, api, admin_token, test_user_ids):
        r = api("GET", f"/audit/logs/{NON_EXISTENT_ID}", token=admin_token)
        _assert_error_body(r, "NOT_FOUND", 404)

    def test_unauthenticated_get_audit_log_returns_401(self, api):
        r = api("GET", f"/audit/logs/{NON_EXISTENT_ID}")
        _assert_error_body(r, "UNAUTHORIZED", 401)

    def test_dispatcher_cannot_read_audit_log_detail(self, api, dispatcher_token, test_user_ids):
        r = api("GET", f"/audit/logs/{NON_EXISTENT_ID}", token=dispatcher_token)
        _assert_error_body(r, "FORBIDDEN", 403)

    def test_finance_cannot_read_audit_log_detail(self, api, finance_token, test_user_ids):
        r = api("GET", f"/audit/logs/{NON_EXISTENT_ID}", token=finance_token)
        _assert_error_body(r, "FORBIDDEN", 403)

    def test_staff_cannot_read_audit_log_detail(self, api, staff_token, test_user_ids):
        r = api("GET", f"/audit/logs/{NON_EXISTENT_ID}", token=staff_token)
        _assert_error_body(r, "FORBIDDEN", 403)
