"""
pytest fixtures for TransitOps API tests.

Setup strategy
--------------
1. Connect to PostgreSQL directly to create test users with argon2id-hashed
   passwords (using the same params as the Rust code: m=19456, t=2, p=1).
2. Use the /auth/login endpoint to obtain session tokens for each role.
3. Yield tokens to test modules.
4. Tear down: delete all rows created during the session.

Environment variables (with defaults for docker-compose)
--------------------------------------------------------------
    API_URL      http://api:8081
  DATABASE_URL postgresql://transitops_app:transitops_secret@db:5432/transitops
  ENCRYPTION_KEY 0123456789abcdef...  (64 hex chars)
"""

import os
import time
import uuid
import struct

import psycopg2
import pytest
import requests
from argon2 import PasswordHasher
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

# ── Configuration ─────────────────────────────────────────────────────────────

API_URL = os.getenv("API_URL", "http://api:8081")
DATABASE_URL = os.getenv(
    "DATABASE_URL",
    "postgresql://transitops_app:transitops_secret@db:5432/transitops",
)
ENCRYPTION_KEY_HEX = os.getenv(
    "ENCRYPTION_KEY",
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
)

# Argon2id with the same defaults as the Rust argon2 0.5 library
_ph = PasswordHasher(
    time_cost=2,
    memory_cost=19456,
    parallelism=1,
    hash_len=32,
    salt_len=16,
)

# AES-256-GCM for field encryption (matches crypto::FieldEncryptor in Rust)
_key = bytes.fromhex(ENCRYPTION_KEY_HEX)
_aesgcm = AESGCM(_key)


def _encrypt_field(plaintext: str) -> bytes:
    """Encrypt a field using AES-256-GCM matching the Rust FieldEncryptor format."""
    nonce = os.urandom(12)
    ciphertext = _aesgcm.encrypt(nonce, plaintext.encode("utf-8"), None)
    return nonce + ciphertext  # nonce(12) || ciphertext+tag(n+16)


# ── Test user definitions ─────────────────────────────────────────────────────

TEST_USERS = {
    "admin": {
        "username": "api_test_admin",
        "password": "TestAdmin123!",
        "role":     "operations_admin",
        "email":    "api_test_admin@transitops.local",
        "fullname": "API Test Admin",
    },
    "finance": {
        "username": "api_test_finance",
        "password": "TestFinance123!",
        "role":     "finance_analyst",
        "email":    "api_test_finance@transitops.local",
        "fullname": "API Test Finance",
    },
    "dispatcher": {
        "username": "api_test_dispatcher",
        "password": "TestDispatch123!",
        "role":     "dispatcher",
        "email":    "api_test_dispatcher@transitops.local",
        "fullname": "API Test Dispatcher",
    },
    "staff": {
        "username": "api_test_staff",
        "password": "TestStaff123!",
        "role":     "staff_user",
        "email":    "api_test_staff@transitops.local",
        "fullname": "API Test Staff",
    },
}


# ── Database helpers ──────────────────────────────────────────────────────────

def _pg_connect():
    """Connect to PostgreSQL, retrying for up to 30 seconds.

    Tries both the configured DATABASE_URL and an automatic hostname
    alternative so the test suite works whether it runs on the Docker
    host (localhost:5432) or inside the Docker network (db:5432).
    """
    urls = [DATABASE_URL]
    # Inside a Docker container, 'localhost' won't reach the db service;
    # add 'db' as a fallback so tests work in both environments.
    if "@localhost:" in DATABASE_URL:
        urls.append(DATABASE_URL.replace("@localhost:", "@db:"))
    elif "@db:" in DATABASE_URL:
        urls.append(DATABASE_URL.replace("@db:", "@localhost:"))

    last_error = None
    for attempt in range(10):
        for url in urls:
            try:
                return psycopg2.connect(url)
            except psycopg2.OperationalError as e:
                last_error = e
        if attempt < 9:
            time.sleep(3)
    raise last_error


def _wait_for_api(max_wait: int = 60):
    """Block until the API is accepting connections or timeout."""
    deadline = time.time() + max_wait
    while time.time() < deadline:
        try:
            r = requests.get(f"{API_URL}/auth/session", timeout=2)
            # 401 means the API is up (just not authenticated)
            if r.status_code in (200, 401):
                return
        except requests.exceptions.ConnectionError:
            time.sleep(1)
    raise RuntimeError(f"API at {API_URL} did not become ready within {max_wait}s")


def _create_test_users(conn) -> dict[str, uuid.UUID]:
    """Insert test users into auth.users; return {role_key: user_id}."""
    user_ids: dict[str, uuid.UUID] = {}
    with conn.cursor() as cur:
        for role_key, spec in TEST_USERS.items():
            user_id = uuid.uuid4()
            pw_hash = _ph.hash(spec["password"])
            email_enc = _encrypt_field(spec["email"])
            cur.execute(
                """
                SELECT id FROM auth.roles WHERE name = %s
                """,
                (spec["role"],),
            )
            row = cur.fetchone()
            if row is None:
                raise RuntimeError(f"Role '{spec['role']}' not found in DB — seeds may not have run")
            role_id = row[0]

            # Upsert so the fixture is idempotent across test runs
            cur.execute(
                """
                INSERT INTO auth.users
                    (id, username, email_encrypted,
                     password_hash, role_id, is_active)
                VALUES (%s, %s, %s, %s, %s, TRUE)
                ON CONFLICT (username) DO UPDATE
                    SET password_hash       = EXCLUDED.password_hash,
                        email_encrypted     = EXCLUDED.email_encrypted,
                        role_id             = EXCLUDED.role_id,
                        is_active           = TRUE,
                        deleted_at          = NULL,
                        updated_at          = now()
                RETURNING id
                """,
                (str(user_id), spec["username"], email_enc, pw_hash, str(role_id)),
            )
            returned = cur.fetchone()[0]
            user_ids[role_key] = returned

    conn.commit()
    return user_ids


def _delete_test_users(conn):
    """Deactivate test users to avoid FK teardown failures."""
    usernames = [spec["username"] for spec in TEST_USERS.values()]
    with conn.cursor() as cur:
        cur.execute(
            """
            DELETE FROM auth.sessions
            WHERE user_id IN (
                SELECT id FROM auth.users WHERE username = ANY(%s)
            )
            """,
            (usernames,),
        )
        cur.execute(
            """
            UPDATE auth.users
            SET is_active = FALSE,
                deleted_at = now(),
                updated_at = now()
            WHERE username = ANY(%s)
            """,
            (usernames,),
        )
    conn.commit()


def _login(username: str, password: str) -> str:
    """Obtain a session token from the API."""
    resp = requests.post(
        f"{API_URL}/auth/login",
        json={"username": username, "password": password},
        timeout=15,
    )
    assert resp.status_code == 200, (
        f"Login failed for {username}: {resp.status_code} {resp.text}"
    )
    data = resp.json()
    return data["token"]


def _reauth(token: str, password: str) -> None:
    """Perform re-authentication on an existing session token.

    This stamps ``last_reauth_at`` on the session so that ReauthGuard-gated
    endpoints can be reached during RBAC tests.  The reauth window is 10
    minutes; as long as the full test suite finishes within that window the
    session-level tokens remain valid for all reauth-gated RBAC checks.
    """
    resp = requests.post(
        f"{API_URL}/auth/reauth",
        headers={"Authorization": f"Bearer {token}"},
        json={"password": password},
        timeout=15,
    )
    # A non-200 response is not fatal here — the RBAC tests that need reauth
    # will simply fail with 403, which makes the problem obvious.
    if resp.status_code != 200:
        import warnings
        warnings.warn(f"Reauth during fixture setup returned {resp.status_code}: {resp.text}")


# ── Session-scoped fixture: shared state across all API tests ─────────────────

@pytest.fixture(scope="session")
def db_conn():
    """A single psycopg2 connection for the entire test session."""
    conn = _pg_connect()
    yield conn
    conn.close()


@pytest.fixture(scope="session")
def test_user_ids(db_conn):
    """Create test users once; yield their DB UUIDs; clean up after."""
    _wait_for_api()
    ids = _create_test_users(db_conn)
    yield ids
    _delete_test_users(db_conn)


@pytest.fixture(scope="session")
def tokens(test_user_ids):
    """Session tokens keyed by role: admin, finance, dispatcher, staff.

    After login, each token is immediately re-authenticated so that
    ReauthGuard-gated endpoints return non-403 responses in RBAC tests.
    The 10-minute reauth window is wide enough for the full test suite.
    """
    result = {}
    for role_key, spec in TEST_USERS.items():
        token = _login(spec["username"], spec["password"])
        _reauth(token, spec["password"])
        result[role_key] = token
    return result


@pytest.fixture(scope="session")
def admin_token(tokens):
    return tokens["admin"]


@pytest.fixture(scope="session")
def finance_token(tokens):
    return tokens["finance"]


@pytest.fixture(scope="session")
def dispatcher_token(tokens):
    return tokens["dispatcher"]


@pytest.fixture(scope="session")
def staff_token(tokens):
    return tokens["staff"]


# ── Per-test request helpers ───────────────────────────────────────────────────

@pytest.fixture(scope="session")
def api():
    """Returns a helper function for making authenticated API calls."""

    def _call(method: str, path: str, token: str | None = None, **kwargs):
        headers = kwargs.pop("headers", {})
        if token:
            headers["Authorization"] = f"Bearer {token}"
        return requests.request(
            method,
            f"{API_URL}{path}",
            headers=headers,
            timeout=10,
            **kwargs,
        )

    return _call
