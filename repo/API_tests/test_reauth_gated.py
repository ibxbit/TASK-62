"""
Reauth-gated endpoint tests.

Tests that privileged admin/finance operations require recent re-authentication
(POST /auth/reauth within the last 10 minutes).

Without a fresh reauth the server must return 403 with code='FORBIDDEN'.
After a successful reauth the operation proceeds (non-403 response).

Endpoints under test
--------------------
  POST /ops/configs/{template_id}/versions/{version_id}/publish
  POST /ops/configs/{template_id}/versions/{version_id}/unpublish
  POST /ops/configs/{template_id}/versions/{version_id}/schedule
  POST /ops/configs/{template_id}/versions/{version_id}/rollout
  POST /reconciliation/runs                     (start a reconciliation run)
  POST /reporting/metrics                       (create metric definition)
  PUT  /reporting/metrics/{id}                  (update metric definition)
  DELETE /reporting/metrics/{id}                (delete metric definition)
  POST /reporting/schedules                     (create report schedule)
  PUT  /reporting/schedules/{id}                (update report schedule)
  DELETE /reporting/schedules/{id}              (delete report schedule)
  POST /reporting/schedules/{id}/trigger        (trigger report run)
  GET  /reporting/runs/{id}/export              (export report run)

Design
------
Each test logs in as the appropriate role through a fresh token that has NOT
had /auth/reauth called.  Sessions issued by /auth/login do NOT set
last_reauth_at, so the ReauthGuard will reject them with 403.

Tests are written to work even when the target resource does not exist
(UUID zeros) — the reauth check fires BEFORE any resource lookup.
"""

import uuid

import pytest

from conftest import TEST_USERS, API_URL
import requests

NON_EXISTENT_ID = "00000000-0000-0000-0000-000000000000"


def _fresh_token_no_reauth(api, role_key: str) -> str:
    """Return a fresh session token that has NOT had /auth/reauth called."""
    spec = TEST_USERS[role_key]
    r = api("POST", "/auth/login",
            json={"username": spec["username"], "password": spec["password"]})
    assert r.status_code == 200, f"Login failed: {r.text}"
    return r.json()["token"]


def _fresh_token_with_reauth(api, role_key: str) -> str:
    """Return a fresh session token after performing /auth/reauth."""
    spec = TEST_USERS[role_key]
    token = _fresh_token_no_reauth(api, role_key)
    r = api("POST", "/auth/reauth", token=token,
            json={"password": spec["password"]})
    assert r.status_code == 200, f"Reauth failed: {r.text}"
    return token


# ── Helpers ───────────────────────────────────────────────────────────────────

def assert_reauth_required(r):
    """Assert response is 403 with FORBIDDEN code — reauth gate fired."""
    assert r.status_code == 403, (
        f"Expected 403 (reauth required), got {r.status_code}: {r.text}"
    )
    body = r.json()
    assert body.get("code") == "FORBIDDEN", (
        f"Expected code=FORBIDDEN, got: {body}"
    )


def assert_reauth_passed(r):
    """Assert response is NOT 403 — reauth gate was satisfied."""
    assert r.status_code != 403, (
        f"Expected non-403 after reauth, got {r.status_code}: {r.text}"
    )


# ── Ops config — publish ─────────────────────────────────────────────────────

class TestPublishReauth:
    _path = f"/ops/configs/{NON_EXISTENT_ID}/versions/{NON_EXISTENT_ID}/publish"

    def test_publish_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "admin")
        r = api("POST", self._path, token=token)
        assert_reauth_required(r)

    def test_publish_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "admin")
        r = api("POST", self._path, token=token)
        assert_reauth_passed(r)


class TestUnpublishReauth:
    _path = f"/ops/configs/{NON_EXISTENT_ID}/versions/{NON_EXISTENT_ID}/unpublish"

    def test_unpublish_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "admin")
        r = api("POST", self._path, token=token)
        assert_reauth_required(r)

    def test_unpublish_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "admin")
        r = api("POST", self._path, token=token)
        assert_reauth_passed(r)


class TestScheduleVersionReauth:
    _path = f"/ops/configs/{NON_EXISTENT_ID}/versions/{NON_EXISTENT_ID}/schedule"

    def test_schedule_without_reauth_returns_403(self, api, test_user_ids):
        import datetime
        token = _fresh_token_no_reauth(api, "admin")
        future = (datetime.datetime.utcnow() + datetime.timedelta(hours=1)).isoformat() + "Z"
        r = api("POST", self._path, token=token,
                json={"effective_from": future})
        assert_reauth_required(r)

    def test_schedule_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        import datetime
        token = _fresh_token_with_reauth(api, "admin")
        future = (datetime.datetime.utcnow() + datetime.timedelta(hours=1)).isoformat() + "Z"
        r = api("POST", self._path, token=token,
                json={"effective_from": future})
        assert_reauth_passed(r)


class TestRolloutReauth:
    _path = f"/ops/configs/{NON_EXISTENT_ID}/versions/{NON_EXISTENT_ID}/rollout"

    def test_rollout_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "admin")
        r = api("POST", self._path, token=token,
                json={"stages": [{"target_percentage": 100, "depot_ids": [NON_EXISTENT_ID]}]})
        assert_reauth_required(r)

    def test_rollout_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "admin")
        r = api("POST", self._path, token=token,
                json={"stages": [{"target_percentage": 100, "depot_ids": [NON_EXISTENT_ID]}]})
        assert_reauth_passed(r)


# ── Reconciliation — start run ────────────────────────────────────────────────

class TestReconciliationRunReauth:
    def test_start_run_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "finance")
        r = api("POST", "/reconciliation/runs", token=token,
                json={
                    "statement_import_id": NON_EXISTENT_ID,
                    "run_date": "2024-01-15",
                })
        assert_reauth_required(r)

    def test_start_run_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "finance")
        r = api("POST", "/reconciliation/runs", token=token,
                json={
                    "statement_import_id": NON_EXISTENT_ID,
                    "run_date": "2024-01-15",
                })
        assert_reauth_passed(r)


# ── Reporting — metric create/update/delete ───────────────────────────────────

class TestMetricCreateReauth:
    def test_create_metric_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "admin")
        r = api("POST", "/reporting/metrics", token=token,
                json={"metric_key": "test", "display_name": "Test", "formula_type": "custom_sql"})
        assert_reauth_required(r)

    def test_create_metric_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "admin")
        r = api("POST", "/reporting/metrics", token=token,
                json={"metric_key": f"test_{uuid.uuid4().hex[:8]}",
                      "display_name": "Test Metric",
                      "formula_type": "custom_sql"})
        assert_reauth_passed(r)


class TestMetricUpdateReauth:
    def test_update_metric_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "admin")
        r = api("PUT", f"/reporting/metrics/{NON_EXISTENT_ID}", token=token,
                json={"display_name": "Updated"})
        assert_reauth_required(r)

    def test_update_metric_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "admin")
        r = api("PUT", f"/reporting/metrics/{NON_EXISTENT_ID}", token=token,
                json={"display_name": "Updated"})
        assert_reauth_passed(r)


class TestMetricDeleteReauth:
    def test_delete_metric_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "admin")
        r = api("DELETE", f"/reporting/metrics/{NON_EXISTENT_ID}", token=token)
        assert_reauth_required(r)

    def test_delete_metric_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "admin")
        r = api("DELETE", f"/reporting/metrics/{NON_EXISTENT_ID}", token=token)
        assert_reauth_passed(r)


# ── Reporting — schedule create/update/delete ─────────────────────────────────

class TestScheduleCreateReauth:
    def test_create_schedule_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "admin")
        r = api("POST", "/reporting/schedules", token=token,
                json={"name": "Test", "metric_ids": [NON_EXISTENT_ID], "schedule": "daily"})
        assert_reauth_required(r)

    def test_create_schedule_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "admin")
        r = api("POST", "/reporting/schedules", token=token,
                json={"name": f"Test {uuid.uuid4().hex[:6]}",
                      "metric_ids": [NON_EXISTENT_ID],
                      "schedule": "daily"})
        assert_reauth_passed(r)


class TestScheduleDeleteReauth:
    def test_delete_schedule_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "admin")
        r = api("DELETE", f"/reporting/schedules/{NON_EXISTENT_ID}", token=token)
        assert_reauth_required(r)

    def test_delete_schedule_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "admin")
        r = api("DELETE", f"/reporting/schedules/{NON_EXISTENT_ID}", token=token)
        assert_reauth_passed(r)


# ── Reporting — trigger run / export ─────────────────────────────────────────

class TestTriggerRunReauth:
    def test_trigger_run_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "admin")
        r = api("POST", f"/reporting/schedules/{NON_EXISTENT_ID}/trigger", token=token)
        assert_reauth_required(r)

    def test_trigger_run_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "admin")
        r = api("POST", f"/reporting/schedules/{NON_EXISTENT_ID}/trigger", token=token)
        assert_reauth_passed(r)


class TestExportRunReauth:
    def test_export_run_without_reauth_returns_403(self, api, test_user_ids):
        token = _fresh_token_no_reauth(api, "admin")
        r = api("GET", f"/reporting/runs/{NON_EXISTENT_ID}/export", token=token,
                params={"format": "csv"})
        assert_reauth_required(r)

    def test_export_run_after_reauth_passes_reauth_gate(self, api, test_user_ids):
        token = _fresh_token_with_reauth(api, "admin")
        r = api("GET", f"/reporting/runs/{NON_EXISTENT_ID}/export", token=token,
                params={"format": "csv"})
        assert_reauth_passed(r)
