"""
Anti-replay signature verification — pure unit tests.

Mirrors the Rust pure functions in `src/payments/signature.rs`.

Functions under test:
  - build_signed_string  → constructs the canonical signed payload
  - hmac_sha256_hex      → HMAC-SHA256
  - hmac_sha512_hex      → HMAC-SHA512
  - sha256_hex           → SHA-256 body hash
  - validate_timestamp   → ±5-minute window check
"""

import hashlib
import hmac
import time


# ── Replicated pure functions ─────────────────────────────────────────────────

MAX_TIMESTAMP_SKEW_SECS = 300  # 5 minutes — matches Rust constant


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def hmac_sha256_hex(secret: bytes, message: bytes) -> str:
    return hmac.new(secret, message, hashlib.sha256).hexdigest()


def hmac_sha512_hex(secret: bytes, message: bytes) -> str:
    return hmac.new(secret, message, hashlib.sha512).hexdigest()


def build_signed_string(nonce: str, timestamp: int, body: bytes, ts_in_sig: bool) -> str:
    body_hash = sha256_hex(body)
    if ts_in_sig:
        return f"{nonce}.{timestamp}.{body_hash}"
    return f"{nonce}.{body_hash}"


def validate_timestamp(ts_secs: int) -> bool:
    """Returns True if timestamp is within ±MAX_TIMESTAMP_SKEW_SECS of now."""
    now = int(time.time())
    return abs(now - ts_secs) <= MAX_TIMESTAMP_SKEW_SECS


def constant_time_eq(a: str, b: str) -> bool:
    return hmac.compare_digest(a.encode(), b.encode())


# ── MAX constant ──────────────────────────────────────────────────────────────

class TestConstant:
    def test_max_skew_is_five_minutes(self):
        assert MAX_TIMESTAMP_SKEW_SECS == 300


# ── Timestamp window ──────────────────────────────────────────────────────────

class TestValidateTimestamp:
    def test_current_time_accepted(self):
        assert validate_timestamp(int(time.time()))

    def test_within_window_accepted(self):
        now = int(time.time())
        assert validate_timestamp(now - MAX_TIMESTAMP_SKEW_SECS)
        assert validate_timestamp(now + MAX_TIMESTAMP_SKEW_SECS)

    def test_one_second_past_stale_rejected(self):
        now = int(time.time())
        assert not validate_timestamp(now - MAX_TIMESTAMP_SKEW_SECS - 1)

    def test_one_second_past_future_rejected(self):
        now = int(time.time())
        assert not validate_timestamp(now + MAX_TIMESTAMP_SKEW_SECS + 1)

    def test_epoch_zero_rejected(self):
        assert not validate_timestamp(0)

    def test_far_past_rejected(self):
        # 2 hours ago
        assert not validate_timestamp(int(time.time()) - 7200)


# ── Signed-string construction ────────────────────────────────────────────────

class TestBuildSignedString:
    def test_with_ts_has_three_dot_parts(self):
        s = build_signed_string("nonce-abc", 1_700_000_000, b"payload", True)
        parts = s.split(".", 2)
        assert len(parts) == 3
        assert parts[0] == "nonce-abc"
        assert parts[1] == "1700000000"
        assert len(parts[2]) == 64  # SHA-256 hex

    def test_without_ts_has_two_dot_parts(self):
        s = build_signed_string("nonce-abc", 1_700_000_000, b"payload", False)
        parts = s.split(".", 1)
        assert len(parts) == 2
        assert "1700000000" not in s

    def test_body_change_changes_signed_string(self):
        s1 = build_signed_string("n", 100, b"original", True)
        s2 = build_signed_string("n", 100, b"tampered", True)
        assert s1 != s2

    def test_nonce_change_changes_signed_string(self):
        s1 = build_signed_string("nonce-A", 100, b"body", True)
        s2 = build_signed_string("nonce-B", 100, b"body", True)
        assert s1 != s2

    def test_timestamp_change_changes_string_when_ts_in_sig(self):
        s1 = build_signed_string("n", 1000, b"body", True)
        s2 = build_signed_string("n", 2000, b"body", True)
        assert s1 != s2

    def test_timestamp_irrelevant_when_ts_not_in_sig(self):
        s1 = build_signed_string("n", 1000, b"body", False)
        s2 = build_signed_string("n", 2000, b"body", False)
        assert s1 == s2

    def test_single_byte_body_change(self):
        s1 = build_signed_string("n", 100, b"Amount=100.00", True)
        s2 = build_signed_string("n", 100, b"Amount=100.01", True)
        assert s1 != s2


# ── HMAC-SHA256 ───────────────────────────────────────────────────────────────

class TestHmacSha256:
    def test_is_deterministic(self):
        assert hmac_sha256_hex(b"key", b"msg") == hmac_sha256_hex(b"key", b"msg")

    def test_output_is_64_hex_chars(self):
        h = hmac_sha256_hex(b"key", b"message")
        assert len(h) == 64
        assert all(c in "0123456789abcdef" for c in h)

    def test_key_sensitive(self):
        h1 = hmac_sha256_hex(b"key-a", b"msg")
        h2 = hmac_sha256_hex(b"key-b", b"msg")
        assert h1 != h2

    def test_message_sensitive(self):
        h1 = hmac_sha256_hex(b"key", b"original")
        h2 = hmac_sha256_hex(b"key", b"tampered")
        assert h1 != h2

    def test_empty_message_does_not_raise(self):
        h = hmac_sha256_hex(b"key", b"")
        assert len(h) == 64


# ── HMAC-SHA512 ───────────────────────────────────────────────────────────────

class TestHmacSha512:
    def test_is_deterministic(self):
        assert hmac_sha512_hex(b"key", b"msg") == hmac_sha512_hex(b"key", b"msg")

    def test_output_is_128_hex_chars(self):
        h = hmac_sha512_hex(b"key", b"message")
        assert len(h) == 128
        assert all(c in "0123456789abcdef" for c in h)

    def test_differs_from_sha256(self):
        h256 = hmac_sha256_hex(b"key", b"msg")
        h512 = hmac_sha512_hex(b"key", b"msg")
        assert h256 != h512


# ── SHA-256 body hash ─────────────────────────────────────────────────────────

class TestSha256:
    def test_empty_body_known_constant(self):
        h = sha256_hex(b"")
        assert h.startswith("e3b0c442")
        assert len(h) == 64

    def test_deterministic(self):
        assert sha256_hex(b"test") == sha256_hex(b"test")

    def test_case_sensitive(self):
        assert sha256_hex(b"body") != sha256_hex(b"Body")

    def test_output_is_64_hex_chars(self):
        assert len(sha256_hex(b"anything")) == 64


# ── Constant-time comparison ──────────────────────────────────────────────────

class TestConstantTimeEq:
    def test_equal_strings(self):
        assert constant_time_eq("abc", "abc")

    def test_different_strings(self):
        assert not constant_time_eq("abc", "def")

    def test_different_lengths(self):
        assert not constant_time_eq("ab", "abc")

    def test_empty_strings_equal(self):
        assert constant_time_eq("", "")


# ── Full verify pipeline (unit-level logic) ────────────────────────────────────

class TestVerifyPipeline:
    """Demonstrates the full verification pipeline without a database."""

    SECRET = b"gateway-webhook-secret"

    def _make_sig(self, nonce: str, ts: int, body: bytes, ts_in_sig: bool) -> str:
        signed = build_signed_string(nonce, ts, body, ts_in_sig)
        return hmac_sha256_hex(self.SECRET, signed.encode())

    def test_valid_signature_matches(self):
        now = int(time.time())
        body = b'{"event":"payment.captured","amount":100}'
        sig = self._make_sig("nonce-1", now, body, True)
        # Re-compute expected and compare
        signed = build_signed_string("nonce-1", now, body, True)
        expected = hmac_sha256_hex(self.SECRET, signed.encode())
        assert constant_time_eq(sig, expected)

    def test_tampered_body_fails(self):
        now = int(time.time())
        original_body = b'{"amount":100}'
        tampered_body = b'{"amount":999}'
        sig = self._make_sig("nonce-2", now, original_body, True)
        signed_tampered = build_signed_string("nonce-2", now, tampered_body, True)
        expected_tampered = hmac_sha256_hex(self.SECRET, signed_tampered.encode())
        assert not constant_time_eq(sig, expected_tampered)

    def test_wrong_secret_fails(self):
        now = int(time.time())
        body = b"data"
        sig = self._make_sig("nonce-3", now, body, True)
        wrong_secret = b"wrong-secret"
        signed = build_signed_string("nonce-3", now, body, True)
        expected_wrong = hmac_sha256_hex(wrong_secret, signed.encode())
        assert not constant_time_eq(sig, expected_wrong)
