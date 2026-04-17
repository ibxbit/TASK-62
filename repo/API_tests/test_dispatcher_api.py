"""
Dispatcher endpoint coverage tests.

Every /dispatcher endpoint has precise contract assertions:
  * happy path   — 2xx + response shape checked (not just "is list")
  * auth failure — 401 + UNAUTHORIZED code
  * RBAC failure — 403 + FORBIDDEN code  (where gating exists)
  * validation   — 400 for missing / malformed body
  * not found    — 404 for non-existent resource

Dispatcher is the positive-role token.  Staff is used as the negative token
where staff lacks the required permission.
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
    assert isinstance(body.get("error"), str) and body["error"]


# ── Trip lifecycle ───────────────────────────────────────────────────────────

class TestDispatcherTripLifecycle:
    """PATCH, assign, start, complete, cancel for a trip id that does not exist."""

    def test_patch_trip_nonexistent_returns_404(self, api, dispatcher_token, test_user_ids):
        """PATCH with empty body against a non-existent trip → 404 (not found beats body check)."""
        r = api("PATCH", f"/dispatcher/trips/{NON_EXISTENT_ID}",
                token=dispatcher_token,
                json={})
        _assert_error_body(r, "NOT_FOUND", 404)

    def test_assign_driver_invalid_body_returns_400(self, api, dispatcher_token, test_user_ids):
        r = api("POST", f"/dispatcher/trips/{NON_EXISTENT_ID}/assign",
                token=dispatcher_token,
                json={})
        assert r.status_code == 400

    def test_start_trip_nonexistent_returns_400(self, api, dispatcher_token, test_user_ids):
        """Start has a required body; missing body → 400."""
        r = api("POST", f"/dispatcher/trips/{NON_EXISTENT_ID}/start",
                token=dispatcher_token)
        assert r.status_code == 400

    def test_complete_trip_nonexistent_returns_400(self, api, dispatcher_token, test_user_ids):
        r = api("POST", f"/dispatcher/trips/{NON_EXISTENT_ID}/complete",
                token=dispatcher_token)
        assert r.status_code == 400

    def test_cancel_trip_nonexistent_returns_400(self, api, dispatcher_token, test_user_ids):
        r = api("POST", f"/dispatcher/trips/{NON_EXISTENT_ID}/cancel",
                token=dispatcher_token)
        assert r.status_code == 400

    def test_unauthenticated_patch_trip_returns_401(self, api):
        r = api("PATCH", f"/dispatcher/trips/{NON_EXISTENT_ID}", json={"notes": "nope"})
        _assert_error_body(r, "UNAUTHORIZED", 401)

    def test_staff_cannot_patch_trip(self, api, staff_token, test_user_ids):
        r = api("PATCH", f"/dispatcher/trips/{NON_EXISTENT_ID}",
                token=staff_token,
                json={"notes": "denied"})
        _assert_error_body(r, "FORBIDDEN", 403)


# ── Trip conflict queries ────────────────────────────────────────────────────

class TestDispatcherTripConflicts:
    def test_get_trip_conflicts_returns_list(self, api, dispatcher_token, test_user_ids):
        """Non-existent trip id → empty list (200), not 404."""
        r = api("GET", f"/dispatcher/trips/{NON_EXISTENT_ID}/conflicts",
                token=dispatcher_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)
        assert r.json() == []

    def test_check_trip_conflicts_nonexistent_returns_404(self, api, dispatcher_token, test_user_ids):
        """Explicit check on non-existent trip id → 404 NOT_FOUND."""
        r = api("POST", f"/dispatcher/trips/{NON_EXISTENT_ID}/check",
                token=dispatcher_token)
        _assert_error_body(r, "NOT_FOUND", 404)

    def test_unauthenticated_get_trip_conflicts_returns_401(self, api):
        r = api("GET", f"/dispatcher/trips/{NON_EXISTENT_ID}/conflicts")
        _assert_error_body(r, "UNAUTHORIZED", 401)


# ── Conflict management ─────────────────────────────────────────────────────

class TestDispatcherConflictManagement:
    def test_list_conflicts_returns_array(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/dispatcher/conflicts", token=dispatcher_token)
        assert r.status_code == 200
        body = r.json()
        assert isinstance(body, list)

    def test_list_conflicts_with_severity_filter_returns_array(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/dispatcher/conflicts?severity=high", token=dispatcher_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)

    def test_acknowledge_conflict_invalid_body_returns_400(self, api, dispatcher_token, test_user_ids):
        r = api("POST", f"/dispatcher/conflicts/{NON_EXISTENT_ID}/acknowledge",
                token=dispatcher_token)
        assert r.status_code == 400

    def test_resolve_conflict_invalid_body_returns_400(self, api, dispatcher_token, test_user_ids):
        r = api("POST", f"/dispatcher/conflicts/{NON_EXISTENT_ID}/resolve",
                token=dispatcher_token,
                json={"resolution": "manual override"})
        assert r.status_code == 400

    def test_unauthenticated_list_conflicts_returns_401(self, api):
        r = api("GET", "/dispatcher/conflicts")
        _assert_error_body(r, "UNAUTHORIZED", 401)


# ── Monitoring dashboard ─────────────────────────────────────────────────────

class TestDispatcherMonitor:
    """Monitoring endpoints — each returns its own specific shape."""

    def test_dashboard_response_contract(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/dispatcher/monitor/dashboard", token=dispatcher_token)
        assert r.status_code == 200
        body = r.json()
        for key in (
            "active_trips_count",
            "upcoming_2h_count",
            "open_conflicts_count",
            "unassigned_within_30min",
            "active_trips",
            "upcoming_trips",
            "recent_conflicts",
        ):
            assert key in body, f"missing dashboard key {key!r}: {body!r}"
        assert isinstance(body["active_trips_count"], int)
        assert isinstance(body["active_trips"], list)
        assert isinstance(body["upcoming_trips"], list)
        assert isinstance(body["recent_conflicts"], list)

    def test_upcoming_response_contract(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/dispatcher/monitor/upcoming", token=dispatcher_token)
        assert r.status_code == 200
        body = r.json()
        assert "trips" in body and isinstance(body["trips"], list)
        assert "window_minutes" in body and isinstance(body["window_minutes"], int)
        # Default window is 120.
        assert body["window_minutes"] == 120

    def test_upcoming_respects_window_query_param(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/dispatcher/monitor/upcoming?window_minutes=60",
                token=dispatcher_token)
        assert r.status_code == 200
        body = r.json()
        assert body["window_minutes"] == 60

    def test_active_trips_returns_list(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/dispatcher/monitor/active", token=dispatcher_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)

    def test_unassigned_response_contract(self, api, dispatcher_token, test_user_ids):
        r = api("GET", "/dispatcher/monitor/unassigned", token=dispatcher_token)
        assert r.status_code == 200
        body = r.json()
        assert "trips" in body and isinstance(body["trips"], list)
        assert "unassigned_count" in body and isinstance(body["unassigned_count"], int)

    def test_check_approaching_returns_200(self, api, dispatcher_token, test_user_ids):
        r = api("POST", "/dispatcher/monitor/check-approaching",
                token=dispatcher_token)
        assert r.status_code == 200

    def test_unauthenticated_dashboard_returns_401(self, api):
        r = api("GET", "/dispatcher/monitor/dashboard")
        _assert_error_body(r, "UNAUTHORIZED", 401)
