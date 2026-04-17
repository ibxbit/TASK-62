"""
Reporting endpoint coverage tests — strict response-contract assertions.
"""

import uuid
import pytest

from conftest import TEST_USERS

NON_EXISTENT_ID = "00000000-0000-0000-0000-000000000000"


def _assert_error_body(resp, expected_code: str, status: int):
    assert resp.status_code == status, f"expected {status}, got {resp.status_code}: {resp.text}"
    body = resp.json()
    assert isinstance(body, dict)
    assert body.get("code") == expected_code, f"got {body!r}"


# ── Metric definitions ──────────────────────────────────────────────────────

class TestReportingMetrics:
    def test_list_metrics_returns_array_with_builtin_fields(self, api, admin_token, test_user_ids):
        r = api("GET", "/reporting/metrics", token=admin_token)
        assert r.status_code == 200
        body = r.json()
        assert isinstance(body, list)
        # Seed installs at least the on-time-departure + fare-leakage metrics.
        assert len(body) >= 1
        for m in body:
            for field in ("id", "metric_key", "display_name", "formula_type", "is_active"):
                assert field in m, f"metric missing {field}: {m}"

    def test_get_metric_nonexistent_returns_404(self, api, admin_token, test_user_ids):
        r = api("GET", f"/reporting/metrics/{NON_EXISTENT_ID}", token=admin_token)
        _assert_error_body(r, "NOT_FOUND", 404)

    def test_compute_metrics_invalid_body_returns_400(self, api, finance_token, test_user_ids):
        r = api("POST", "/reporting/metrics/compute", token=finance_token,
                json={"metric_ids": [NON_EXISTENT_ID]})
        # Compute requires metric IDs + date window; minimal body → 400.
        assert r.status_code == 400

    def test_unauthenticated_get_metric_returns_401(self, api):
        r = api("GET", f"/reporting/metrics/{NON_EXISTENT_ID}")
        _assert_error_body(r, "UNAUTHORIZED", 401)

    def test_staff_can_read_metric_list(self, api, staff_token, test_user_ids):
        """ReportingRead is granted to staff_user."""
        r = api("GET", "/reporting/metrics", token=staff_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)


# ── Schedules ───────────────────────────────────────────────────────────────

class TestReportingSchedules:
    def test_list_schedules_returns_array(self, api, admin_token, test_user_ids):
        r = api("GET", "/reporting/schedules", token=admin_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)

    def test_get_schedule_nonexistent_returns_404(self, api, admin_token, test_user_ids):
        r = api("GET", f"/reporting/schedules/{NON_EXISTENT_ID}", token=admin_token)
        _assert_error_body(r, "NOT_FOUND", 404)

    def test_update_schedule_nonexistent_returns_404(self, api, finance_token, test_user_ids):
        r = api("PUT", f"/reporting/schedules/{NON_EXISTENT_ID}",
                token=finance_token,
                json={"name": "Updated Schedule", "cron": "0 0 * * *"})
        _assert_error_body(r, "NOT_FOUND", 404)

    def test_unauthenticated_list_schedules_returns_401(self, api):
        r = api("GET", "/reporting/schedules")
        _assert_error_body(r, "UNAUTHORIZED", 401)

    def test_staff_can_read_schedules(self, api, staff_token, test_user_ids):
        r = api("GET", "/reporting/schedules", token=staff_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)

    def test_staff_cannot_update_schedule(self, api, staff_token, test_user_ids):
        r = api("PUT", f"/reporting/schedules/{NON_EXISTENT_ID}",
                token=staff_token,
                json={"name": "x", "cron": "* * * * *"})
        _assert_error_body(r, "FORBIDDEN", 403)


# ── Runs ─────────────────────────────────────────────────────────────────────

class TestReportingRuns:
    def test_list_runs_returns_array(self, api, admin_token, test_user_ids):
        r = api("GET", "/reporting/runs", token=admin_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)

    def test_get_run_nonexistent_returns_404(self, api, admin_token, test_user_ids):
        r = api("GET", f"/reporting/runs/{NON_EXISTENT_ID}", token=admin_token)
        _assert_error_body(r, "NOT_FOUND", 404)

    def test_unauthenticated_list_runs_returns_401(self, api):
        r = api("GET", "/reporting/runs")
        _assert_error_body(r, "UNAUTHORIZED", 401)

    def test_staff_can_read_runs(self, api, staff_token, test_user_ids):
        r = api("GET", "/reporting/runs", token=staff_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)
