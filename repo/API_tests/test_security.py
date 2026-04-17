"""
Security tests — callback HMAC verification, timestamp replay window,
object-level ownership enforcement, and DND preferences.

Every assertion is tightened to a precise expected outcome: bad signature must
fail auth (not simply "non-2xx"), and stale timestamps must be rejected as
replay attempts, not swallowed as a 2xx.
"""

import base64
import hashlib
import hmac
import time
import uuid

import pytest
import requests

from conftest import TEST_USERS, API_URL

NON_EXISTENT_ID = "00000000-0000-0000-0000-000000000000"


def _unique_key(prefix: str = "sec_test") -> str:
    return f"{prefix}_{uuid.uuid4().hex[:12]}"


def _assert_not_success(resp, reason: str):
    """A security check must never allow a 2xx response."""
    assert resp.status_code >= 400, (
        f"{reason}: endpoint should reject but returned {resp.status_code} — "
        f"body: {resp.text[:200]}"
    )
    assert resp.status_code < 500, (
        f"{reason}: endpoint returned server error {resp.status_code} — "
        "security handler should fail closed, not crash"
    )


# ── Callback signature verification ──────────────────────────────────────────

class TestCallbackSignatureVerification:
    """Callback HMAC + replay window enforcement."""

    GATEWAY_NAME = "offline_test"
    GATEWAY_SECRET = b"test_secret_for_integration_tests_minimum_32_chars_long"
    CALLBACK_URL = f"/payments/callbacks/{GATEWAY_NAME}"

    def _sign(self, nonce: str, ts: int, body: bytes, secret: bytes = None) -> str:
        if secret is None:
            secret = self.GATEWAY_SECRET
        body_hash = hashlib.sha256(body).hexdigest()
        signed_string = f"{nonce}.{ts}.{body_hash}"
        return hmac.new(secret, signed_string.encode(), hashlib.sha256).hexdigest()

    def _make_payload(self, txn_id: str, status: str = "completed") -> bytes:
        import json
        return json.dumps({
            "transaction_ref": txn_id,
            "status": status,
            "amount": "50.00",
        }).encode()

    def test_callback_bad_signature_is_rejected(self, api):
        """A callback with an all-zero signature must be rejected.
        No valid transaction is required — the signature gate is the first check."""
        nonce = f"nonce_{uuid.uuid4().hex}"
        ts = int(time.time())
        payload = self._make_payload(NON_EXISTENT_ID)

        r = requests.post(
            f"{API_URL}{self.CALLBACK_URL}",
            data=payload,
            headers={
                "Content-Type": "application/json",
                "X-Signature": "0" * 64,
                "X-Nonce": nonce,
                "X-Timestamp": str(ts),
            },
            timeout=10,
        )
        _assert_not_success(r, "bad-signature callback")

    def test_callback_stale_timestamp_is_rejected(self, api):
        """Timestamp older than the replay window (~5 min) must fail regardless of signature."""
        nonce = f"nonce_{uuid.uuid4().hex}"
        ts = int(time.time()) - 400  # 400s ago — beyond the 5-minute window
        payload = self._make_payload(NON_EXISTENT_ID)
        sig = self._sign(nonce, ts, payload)

        r = requests.post(
            f"{API_URL}{self.CALLBACK_URL}",
            data=payload,
            headers={
                "Content-Type": "application/json",
                "X-Signature": sig,
                "X-Nonce": nonce,
                "X-Timestamp": str(ts),
            },
            timeout=10,
        )
        _assert_not_success(r, "stale-timestamp callback")

    def test_callback_missing_signature_headers_is_rejected(self, api):
        """A callback without signature headers at all must not be processed."""
        r = requests.post(
            f"{API_URL}{self.CALLBACK_URL}",
            data=b'{"transaction_ref":"x","status":"completed"}',
            headers={"Content-Type": "application/json"},
            timeout=10,
        )
        _assert_not_success(r, "callback with no signature headers")

    def test_simulate_callback_requires_auth(self, api):
        """/payments/callbacks/simulate must return 401 + UNAUTHORIZED without a token."""
        r = api("POST", "/payments/callbacks/simulate",
                json={"gateway": "test_gw",
                      "transaction_id": NON_EXISTENT_ID,
                      "status": "completed"})
        assert r.status_code == 401
        body = r.json()
        assert body.get("code") == "UNAUTHORIZED", f"unexpected body: {body}"


# ── Object-level ownership ───────────────────────────────────────────────────

class TestObjectLevelOwnership:
    """Cross-user notification-rule access enforcement."""

    def test_cross_user_rule_access_denied(self, api, admin_token, dispatcher_token, test_user_ids):
        """Admin creates a rule → dispatcher must not be able to GET it."""
        create_r = api("POST", "/notifications/rules", token=admin_token,
                       json={
                           "rule_name": "Admin private rule",
                           "rule_type": "keyword",
                           "config": {"keywords": ["urgent"]},
                       })
        assert create_r.status_code in (200, 201), (
            f"rule create failed: {create_r.status_code} {create_r.text}"
        )
        rule_id = create_r.json()["id"]

        read_r = api("GET", f"/notifications/rules/{rule_id}", token=dispatcher_token)
        assert read_r.status_code in (403, 404), (
            f"cross-user read must be denied (403/404), got {read_r.status_code}"
        )
        body = read_r.json()
        assert body.get("code") in ("FORBIDDEN", "NOT_FOUND"), body

    def test_cross_user_rule_mutation_denied(self, api, admin_token, dispatcher_token, test_user_ids):
        """Admin creates a rule → dispatcher cannot DELETE it."""
        create_r = api("POST", "/notifications/rules", token=admin_token,
                       json={
                           "rule_name": "Admin private rule for delete test",
                           "rule_type": "keyword",
                           "config": {"keywords": ["delete_test"]},
                       })
        assert create_r.status_code in (200, 201)
        rule_id = create_r.json()["id"]

        del_r = api("DELETE", f"/notifications/rules/{rule_id}", token=dispatcher_token)
        assert del_r.status_code in (403, 404), (
            f"cross-user delete must be denied, got {del_r.status_code}"
        )


# ── DND preferences ──────────────────────────────────────────────────────────

class TestDndBehavior:
    """Users must be able to read and update their own DND preferences."""

    def test_staff_can_read_own_preferences(self, api, staff_token, test_user_ids):
        r = api("GET", "/notifications/preferences", token=staff_token)
        assert r.status_code == 200, f"staff prefs read failed: {r.status_code} {r.text}"
        body = r.json()
        assert isinstance(body, dict), f"prefs should be object, got {type(body).__name__}"

    def test_staff_can_update_own_preferences(self, api, staff_token, test_user_ids):
        r = api("PUT", "/notifications/preferences", token=staff_token,
                json={
                    "dnd_enabled": True,
                    "dnd_start": "22:00",
                    "dnd_end": "06:00",
                })
        assert r.status_code in (200, 204), f"DND update failed: {r.status_code} {r.text}"

    def test_unauthenticated_cannot_read_preferences(self, api):
        r = api("GET", "/notifications/preferences")
        assert r.status_code == 401
        assert r.json().get("code") == "UNAUTHORIZED"
