"""
Reconciliation endpoint coverage tests — statements and run detail endpoints.

Strict contract:
  * list endpoints return a JSON array
  * GET /runs/:id returns a single object (or 404 for unknown id)
  * run detail / summary / items endpoints have precise not-found contract
"""

import pytest

from conftest import TEST_USERS

NON_EXISTENT_ID = "00000000-0000-0000-0000-000000000000"


def _assert_error_body(resp, expected_code: str, status: int):
    assert resp.status_code == status, f"expected {status}, got {resp.status_code}: {resp.text}"
    body = resp.json()
    assert isinstance(body, dict)
    assert body.get("code") == expected_code, f"got {body!r}"


# ── Statements ───────────────────────────────────────────────────────────────

class TestReconciliationStatements:
    def test_list_statements_returns_array(self, api, finance_token, test_user_ids):
        r = api("GET", "/reconciliation/statements", token=finance_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)

    def test_upload_statement_invalid_body_returns_400(self, api, finance_token, test_user_ids):
        r = api("POST", "/reconciliation/statements", token=finance_token,
                json={"source": "bank_api", "period": "2025-03"})
        assert r.status_code == 400
        # Should mention the missing field.
        assert r.text.strip() != ""

    def test_unauthenticated_list_statements_returns_401(self, api):
        r = api("GET", "/reconciliation/statements")
        _assert_error_body(r, "UNAUTHORIZED", 401)

    def test_dispatcher_cannot_list_statements(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/reconciliation/statements", token=dispatcher_token)
        _assert_error_body(r, "FORBIDDEN", 403)

    def test_staff_cannot_upload_statement(self, api, staff_token, test_user_ids):
        r = api("POST", "/reconciliation/statements", token=staff_token,
                json={"source": "bank_api"})
        # staff_user lacks reconciliation write; body may be rejected first.
        assert r.status_code in (400, 403)


# ── Runs detail ──────────────────────────────────────────────────────────────

class TestReconciliationRunDetail:
    def test_get_run_nonexistent_returns_404(self, api, finance_token, test_user_ids):
        r = api("GET", f"/reconciliation/runs/{NON_EXISTENT_ID}",
                token=finance_token)
        _assert_error_body(r, "NOT_FOUND", 404)

    def test_run_summary_nonexistent_returns_404(self, api, finance_token, test_user_ids):
        r = api("GET", f"/reconciliation/runs/{NON_EXISTENT_ID}/summary",
                token=finance_token)
        _assert_error_body(r, "NOT_FOUND", 404)

    def test_run_items_nonexistent_returns_empty_list(self, api, finance_token, test_user_ids):
        """Items endpoint returns an empty list for unknown runs rather than 404."""
        r = api("GET", f"/reconciliation/runs/{NON_EXISTENT_ID}/items",
                token=finance_token)
        assert r.status_code == 200
        assert r.json() == []

    def test_unauthenticated_get_run_returns_401(self, api):
        r = api("GET", f"/reconciliation/runs/{NON_EXISTENT_ID}")
        _assert_error_body(r, "UNAUTHORIZED", 401)

    def test_admin_can_read_run_detail(self, api, admin_token, test_user_ids):
        """operations_admin has PaymentsReconciliationRead — must NOT 403."""
        r = api("GET", f"/reconciliation/runs/{NON_EXISTENT_ID}",
                token=admin_token)
        _assert_error_body(r, "NOT_FOUND", 404)

    def test_staff_cannot_read_run_detail(self, api, staff_token, test_user_ids):
        r = api("GET", f"/reconciliation/runs/{NON_EXISTENT_ID}",
                token=staff_token)
        _assert_error_body(r, "FORBIDDEN", 403)
