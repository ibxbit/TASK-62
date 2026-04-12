# TransitOps

Transit operations platform — Rust/Actix-web API backed by PostgreSQL, with a Yew (Rust/WASM) backoffice frontend.

---

## Start command

```bash
docker compose up
```

That single command:
1. Starts a PostgreSQL 16 container and runs schema migrations + seed data
2. Builds and starts the Rust API server (waits for the database to be healthy)

The first build compiles the Rust binary inside Docker and takes several minutes. Subsequent starts are fast.

To run in the background:

```bash
docker compose up -d
```

To stop and remove containers:

```bash
docker compose down
```

To also remove the database volume (full reset):

```bash
docker compose down -v
```

---

## Service addresses

| Service    | Address                   | Notes                          |
|------------|---------------------------|--------------------------------|
| API        | http://localhost:8081     | REST API, all endpoints        |
| PostgreSQL | localhost:5432            | User `transitops_app`, DB `transitops` |

---

## Step-by-step verification

### 1. Confirm services are running

```bash
docker compose ps
```

Expected output: both `db` and `api` show `running` (or `Up`).

### 2. Confirm the API is reachable

```bash
curl -s http://localhost:8081/auth/session
```

Expected: `{"error":"..."}` or similar with HTTP 401 — this confirms the API is up.

### 3. Log in as the seed admin user

```bash
curl -s -X POST http://localhost:8081/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"AdminPass123!"}' | jq .
```

Expected response:

```json
{
  "token": "<session-token>",
  "username": "admin",
  "role": "operations_admin"
}
```

Save the token:

```bash
TOKEN=$(curl -s -X POST http://localhost:8081/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"AdminPass123!"}' | jq -r .token)
```

### 4. Verify authenticated session

```bash
curl -s http://localhost:8081/auth/session \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected: object with `username`, `role`, `session_id`.

### 5. Check alerts (core domain)

```bash
curl -s http://localhost:8081/alerts \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected: JSON array (may be empty on a fresh instance).

### 6. Check notifications inbox

```bash
curl -s http://localhost:8081/notifications \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### 7. Check payments transactions

```bash
curl -s http://localhost:8081/payments/transactions \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Note: the seed admin has `PaymentsTransactionsRead` but not `Write`. Expect 200.

### 8. Verify RBAC — confirm dispatcher cannot access audit logs

```bash
# Log in as dispatcher
DISPATCHER_TOKEN=$(curl -s -X POST http://localhost:8081/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"dispatcher","password":"DispatcherPass123!"}' | jq -r .token)

curl -s -o /dev/null -w "%{http_code}" \
  http://localhost:8081/audit/logs \
  -H "Authorization: Bearer $DISPATCHER_TOKEN"
# Expected: 403
```

---

## Running the tests

### Prerequisites

- Python 3.10 or later
- pip packages from `requirements.txt`
- The full stack running (`docker compose up -d`)

Install dependencies once:

```bash
pip install -r requirements.txt
```

### Run all tests

```bash
./run_tests.sh
```

### Run unit tests only (no running server required)

```bash
./run_tests.sh unit
```

### Run API tests only

```bash
./run_tests.sh api
```

### Run against a non-default API URL

```bash
API_URL=http://staging.example.com:8081 ./run_tests.sh api
```

### Test suite layout

```
unit_tests/
  test_dnd_logic.py            DND window logic (boundary, midnight-crossing)
  test_reconciliation_logic.py Discrepancy classification, duplicate detection
  test_signature_logic.py      HMAC-SHA256/512, signed-string construction, timestamp window
  test_alert_severity.py       KPI severity thresholds, FNV-1a advisory lock IDs

API_tests/
  conftest.py                  Session fixtures: test users, tokens, api() helper
  test_auth_api.py             Login, session, logout, reauth
  test_notifications_api.py    Inbox, DND prefs, subscriptions, rules, announce
  test_payments_api.py         Transactions, refunds, imports, callbacks
  test_alerting_api.py         List/filter, stats, acknowledge, close
  test_rbac_api.py             RBAC enforcement across all four roles
```

---

## Configuration

All configuration is passed via environment variables (see `docker-compose.yml`).

| Variable                   | Default                                                    | Description                                                    |
|----------------------------|------------------------------------------------------------|----------------------------------------------------------------|
| `DATABASE_URL`             | `postgresql://transitops_app:transitops_secret@db:5432/transitops` | PostgreSQL connection string                         |
| `ENCRYPTION_KEY`           | _(none — required)_                                        | **64 hex characters** (32-byte AES-256-GCM key). Must be exactly 64 hex chars. |
| `ENCRYPTION_KEY_PREVIOUS`  | _(optional)_                                               | Previous 64-hex key, used during key rotation.                 |
| `API_URL`                  | `http://localhost:8081`                                    | Used by test scripts only.                                     |

`ENCRYPTION_KEY` must be exactly **64 hexadecimal characters** representing a 32-byte
AES-256 key.  Generate a secure value with:

```bash
openssl rand -hex 32
```

The server **refuses to start** with a missing or malformed key, so a bad value fails fast.

---

## Frontend (Yew backoffice)

The frontend is a Rust/WASM single-page application built with [Yew](https://yew.rs/).
It covers all role-specific workflows: ops config lifecycle, dispatcher conflicts,
finance reconciliation/refunds, notification inbox/subscriptions, reporting/KPI,
and alerting.

### Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
```

### Development server (with hot-reload)

```bash
cd frontend
trunk serve
```

The frontend dev server starts at `http://localhost:8081` by default (proxied to API on 8081).

### Production build

```bash
cd frontend
trunk build --release
```

Output goes to `frontend/dist/`.  Serve these static files with any web server
(nginx, Caddy, etc.) in front of the API.

### Frontend test suite

Run type-level and logic tests (compiles to native, no browser required):

```bash
cd frontend
cargo test --lib
```

Run the full component integration tests (requires wasm-pack and Firefox or Chrome):

```bash
cd frontend
wasm-pack test --headless --firefox -- --test component_states
```

Tests exercise real production types: `SessionInfo` role checks, rollout page state
transitions, statement import validation, ops admin CRUD state machines, and the
inline base64 encoder used in the file-upload path.

### Entry points

| Role              | Starting route                | Notes                              |
|-------------------|-------------------------------|------------------------------------|
| Operations Admin  | `/ops/config`                 | Config lifecycle (list/diff/rollout/schedule)|
| Operations Admin  | `/ops/routes`                 | Route management (CRUD)            |
| Operations Admin  | `/ops/stops`                  | Stop management per route (CRUD)   |
| Operations Admin  | `/ops/calendars`              | Trip calendar management (CRUD)    |
| Operations Admin  | `/ops/fare-rules`             | Fare rule configuration (CRUD)     |
| Operations Admin  | `/ops/change-refund-rules`    | Change & refund policy configuration (CRUD) |
| Dispatcher        | `/dispatch/trips`             | Trip adjustments                   |
| Dispatcher        | `/dispatch/conflicts`         | Conflict monitor                   |
| Finance Analyst   | `/finance/statements`         | Statement import                   |
| Finance Analyst   | `/finance/reconciliation`     | Reconciliation runs                |
| Finance Analyst   | `/finance/refunds`            | Refund management                  |
| Staff User        | `/notifications`              | Inbox                              |
| All roles         | `/reporting/metrics`          | KPI metrics + drilldown filters    |
| All roles         | `/reporting/schedules`        | Report schedules                   |
| All roles         | `/reporting/runs`             | Report run history + export        |
| Admin/Dispatcher/Finance | `/alerts`            | Alert dashboard                    |
| Operations Admin  | `/alerts/rules`               | Alert rule subscription management (CRUD) |

---

## Project structure

```
repo/
├── src/                  Rust source (Actix-web handlers, domain modules)
├── frontend/             Yew WASM frontend (role-based backoffice)
│   ├── src/
│   │   ├── main.rs       App entry point with router
│   │   ├── pages/        One module per role/workflow area
│   │   ├── components/   Shared UI components (nav, role guard, reauth prompt)
│   │   ├── services/     API client layer (per domain)
│   │   ├── store/        Context-based state management
│   │   └── types/        Shared domain types
│   └── Cargo.toml
├── db/
│   ├── schema.sql        Base schema (roles, tables, indexes)
│   ├── migrations/       Numbered migration files (applied in order)
│   ├── seeds/            Seed data (roles, default admin user)
│   └── init/
│       └── 00_init.sh    PostgreSQL Docker init script
├── unit_tests/           Pure Python unit tests (no server needed)
├── API_tests/            Integration tests against the running API
├── run_tests.sh          Test runner script
├── Dockerfile            Multi-stage build (builder + runtime)
├── docker-compose.yml    Service definitions
└── requirements.txt      Python test dependencies
```
