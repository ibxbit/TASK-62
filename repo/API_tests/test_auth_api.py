"""
Auth endpoint tests — POST /auth/login, POST /auth/logout,
GET /auth/session, POST /auth/reauth.

Coverage
--------
  - Successful login returns token + role
  - Wrong password → 401
  - Unknown user → 401 (same error, no enumeration)
  - Missing fields → 400
  - Valid session GET → 200 with user data
  - Unauthenticated session GET → 401
  - Logout invalidates session
  - Reauth with correct password succeeds
  - Reauth with wrong password fails
"""

import pytest
import requests

from conftest import API_URL, TEST_USERS


# ── Login ─────────────────────────────────────────────────────────────────────

class TestLogin:
    def test_admin_login_succeeds(self, api, test_user_ids):
        spec = TEST_USERS["admin"]
        r = api("POST", "/auth/login",
                json={"username": spec["username"], "password": spec["password"]})
        assert r.status_code == 200
        body = r.json()
        assert "token" in body
        assert body["username"] == spec["username"]
        assert body["role"] == spec["role"]
        assert len(body["token"]) > 20

    def test_all_roles_can_login(self, api, test_user_ids):
        for role_key, spec in TEST_USERS.items():
            r = api("POST", "/auth/login",
                    json={"username": spec["username"], "password": spec["password"]})
            assert r.status_code == 200, f"Login failed for {role_key}: {r.text}"

    def test_wrong_password_returns_401(self, api, test_user_ids):
        spec = TEST_USERS["admin"]
        r = api("POST", "/auth/login",
                json={"username": spec["username"], "password": "WrongPass999!"})
        assert r.status_code == 401
        body = r.json()
        assert "error" in body or "message" in body

    def test_unknown_user_returns_401(self, api):
        r = api("POST", "/auth/login",
                json={"username": "does_not_exist_xyz", "password": "anypassword"})
        assert r.status_code == 401

    def test_unknown_user_same_error_as_wrong_password(self, api, test_user_ids):
        """Prevents user enumeration: same HTTP status for unknown user vs bad password."""
        spec = TEST_USERS["admin"]
        r_wrong_pw = api("POST", "/auth/login",
                         json={"username": spec["username"], "password": "WrongPass!"})
        r_no_user  = api("POST", "/auth/login",
                         json={"username": "no_such_user_xyz", "password": "anypassword"})
        assert r_wrong_pw.status_code == r_no_user.status_code == 401

    def test_missing_username_returns_400(self, api):
        r = api("POST", "/auth/login", json={"password": "whatever"})
        assert r.status_code in (400, 422)

    def test_missing_password_returns_400(self, api):
        r = api("POST", "/auth/login", json={"username": "someone"})
        assert r.status_code in (400, 422)

    def test_empty_body_returns_400(self, api):
        r = api("POST", "/auth/login", json={})
        assert r.status_code in (400, 422)

    def test_response_contains_required_fields(self, api, test_user_ids):
        spec = TEST_USERS["admin"]
        r = api("POST", "/auth/login",
                json={"username": spec["username"], "password": spec["password"]})
        body = r.json()
        for field in ("token", "username", "role"):
            assert field in body, f"Missing field: {field}"


# ── Session ───────────────────────────────────────────────────────────────────

class TestSession:
    def test_authenticated_session_returns_200(self, api, admin_token):
        r = api("GET", "/auth/session", token=admin_token)
        assert r.status_code == 200
        body = r.json()
        assert "username" in body
        assert "role" in body

    def test_unauthenticated_returns_401(self, api):
        r = api("GET", "/auth/session")
        assert r.status_code == 401

    def test_invalid_token_returns_401(self, api):
        r = api("GET", "/auth/session",
                headers={"Authorization": "Bearer this_is_not_a_valid_token"})
        assert r.status_code == 401

    def test_session_username_matches_login(self, api, test_user_ids):
        spec = TEST_USERS["dispatcher"]
        login_r = api("POST", "/auth/login",
                      json={"username": spec["username"], "password": spec["password"]})
        token = login_r.json()["token"]
        session_r = api("GET", "/auth/session", token=token)
        assert session_r.json()["username"] == spec["username"]


# ── Logout ─────────────────────────────────────────────────────────────────────

class TestLogout:
    def test_logout_returns_200(self, api, test_user_ids):
        # Obtain a fresh token for this test
        spec = TEST_USERS["staff"]
        token = api("POST", "/auth/login",
                    json={"username": spec["username"],
                          "password": spec["password"]}).json()["token"]
        r = api("POST", "/auth/logout", token=token)
        assert r.status_code == 200

    def test_token_invalid_after_logout(self, api, test_user_ids):
        spec = TEST_USERS["staff"]
        token = api("POST", "/auth/login",
                    json={"username": spec["username"],
                          "password": spec["password"]}).json()["token"]
        api("POST", "/auth/logout", token=token)
        r = api("GET", "/auth/session", token=token)
        assert r.status_code == 401

    def test_logout_without_auth_returns_401(self, api):
        r = api("POST", "/auth/logout")
        assert r.status_code == 401


# ── Reauth ─────────────────────────────────────────────────────────────────────

class TestReauth:
    def test_reauth_with_correct_password_succeeds(self, api, admin_token, test_user_ids):
        spec = TEST_USERS["admin"]
        r = api("POST", "/auth/reauth", token=admin_token,
                json={"password": spec["password"]})
        assert r.status_code == 200

    def test_reauth_with_wrong_password_fails(self, api, admin_token):
        r = api("POST", "/auth/reauth", token=admin_token,
                json={"password": "WrongPassword999!"})
        assert r.status_code == 401

    def test_reauth_without_auth_returns_401(self, api):
        r = api("POST", "/auth/reauth", json={"password": "anypassword"})
        assert r.status_code == 401
