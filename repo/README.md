# TransitOps

> **Project type:** fullstack

Transit operations platform — Rust/Actix-web API backed by PostgreSQL, with a Yew (Rust/WASM) backoffice frontend.

---

## Start command

```bash
docker-compose up
```

Or with the modern Docker Compose v2 CLI:

```bash
docker compose up
```

That single command:
1. Starts a PostgreSQL 16 container and runs schema migrations + seed data (including demo users)
2. Builds and starts the Rust API server (waits for the database to be healthy)
3. (optional) Adds `--profile test` to also bring up the Yew frontend on port 80

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

| Service    | Address                          | Notes                                    |
|------------|----------------------------------|------------------------------------------|
| Frontend   | **http://localhost:80**          | Yew/WASM SPA — login → inbox             |
| API        | **http://localhost:8081**        | REST API, all endpoints                  |
| PostgreSQL | localhost:5432                   | User `transitops_app`, DB `transitops`   |

> The frontend SPA is built into its own image (`frontend/Dockerfile`) and
> served by nginx, which reverse-proxies API XHRs to the backend container.
> Bring it up with:
>
> ```bash
> docker compose --profile test up frontend-test
> ```

---

## Demo credentials

Four demo users are seeded on first database init (see `db/seeds/010_demo_users_seed.sql`).
**Development/testing only — never use in production.**

| Role              | Username     | Password              | Email                       |
|-------------------|--------------|-----------------------|-----------------------------|
| Operations Admin  | `admin`      | `AdminPass123!`       | admin@transitops.local      |
| Dispatcher        | `dispatcher` | `DispatcherPass123!`  | dispatcher@transitops.local |
| Finance Analyst   | `finance`    | `FinancePass123!`     | finance@transitops.local    |
| Staff User        | `staff`      | `StaffPass123!`       | staff@transitops.local      |

---

## Verification

### Backend verification (curl)

#### 1. Confirm services are running

```bash
docker compose ps
```

Expected output: both `db` and `api` show `running` (or `Up`).

#### 2. Confirm the API is reachable

```bash
curl -s http://localhost:8081/auth/session
```

Expected: `{"error":"..."}` or similar with HTTP 401 — this confirms the API is up.

#### 3. Log in as the seed admin user

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

#### 4. Verify authenticated session

```bash
curl -s http://localhost:8081/auth/session \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected: object with `username`, `role`, `session_id`.

#### 5. Check alerts (core domain)

```bash
curl -s http://localhost:8081/alerts \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected: JSON array (may be empty on a fresh instance).

#### 6. Check notifications inbox

```bash
curl -s http://localhost:8081/notifications \
  -H "Authorization: Bearer $TOKEN" | jq .
```

#### 7. Check payments transactions

```bash
curl -s http://localhost:8081/payments/transactions \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Note: the seed admin has `PaymentsTransactionsRead` but not `Write`. Expect 200.

#### 8. Verify RBAC — confirm dispatcher cannot access audit logs

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

### Web UI verification (browser)

Make sure the frontend is running:

```bash
docker compose --profile test up -d frontend-test api-test db-test
```

Then open **http://localhost:80** and walk through these steps:

1. **Login page** — the landing page at `/login` displays the TransitOps card.
   Enter `admin` / `AdminPass123!` and click **Sign in**.
2. **Inbox** — after login you are redirected to `/notifications`; the "Inbox"
   heading is visible. Click **Mark all read** to exercise the inbox POST flow.
3. **Alerts dashboard** — navigate to `/alerts`. The dashboard heading shows and
   the page calls `GET /alerts` through the real API (reverse-proxied by nginx).
4. **Reporting metrics** — navigate to `/reporting/metrics`. The seeded KPI
   metric **On-Time Departure Rate** (from seed 005) is rendered.
5. **RBAC guard** — log out, sign in as `staff` / `StaffPass123!`, then try to
   visit `/alerts`. The backend returns 403 and the SPA shows a permission
   error without leaking alert data.

The Playwright E2E suite in `e2e/tests/` automates these exact flows in
Chromium — run them headlessly with `./run_tests.sh e2e`.

---

## Running the tests

All tests run inside Docker containers — there is **no host requirement**
beyond Docker + Docker Compose v2. No Python, pip, Rust, wasm-pack, Node, or
browsers need to be installed locally. The same `./run_tests.sh` invocation
works on a developer laptop and in a CI runner.

### Prerequisites

- Docker 20.10+
- Docker Compose v2 (`docker compose version`)

That's it.

### Run everything (unit, API, integration, frontend, E2E)

```bash
./run_tests.sh            # equivalent to: ./run_tests.sh all
```

### Run a single category

```bash
./run_tests.sh unit         # pure-Python unit tests, no services needed
./run_tests.sh api          # Python API tests against a disposable db + api
./run_tests.sh integration  # cargo test --tests against a disposable db
./run_tests.sh frontend     # Yew wasm-pack tests in headless Firefox
./run_tests.sh e2e          # Playwright through nginx → WASM → API → Postgres
```

### How isolation works

The `test` profile in `docker-compose.yml` defines a parallel stack —
`db-test`, `api-test`, `frontend-test` — that never touches the developer
`db` / `api` services or the persistent `transitops_db_data` volume.

* `db-test` mounts `/var/lib/postgresql/data` on a `tmpfs`, so all data
  vanishes when the container dies.
* `api-test` and `frontend-test` are rebuilt from the same `Dockerfile` /
  `frontend/Dockerfile` that ship to production, so tests exercise the real
  binary, not a development build.
* `KEEP_STACK=1 ./run_tests.sh api` leaves the containers running between
  invocations for fast iteration; otherwise the script tears them down on
  exit (named build-cache volumes are preserved).

### End-to-end tests (Playwright)

`./run_tests.sh e2e` boots the entire stack — disposable Postgres, the
production-shaped Rust API, and the Yew SPA served through nginx — then
launches Playwright (Chromium) inside its own container to drive a real
browser through four meaningful flows. **No mocks are used at any layer**;
every test hits the real WASM SPA → real nginx → real API → real Postgres.

| Spec file | User flow exercised |
|-----------|---------------------|
| `e2e/tests/login.spec.ts` | Login with seeded admin → token persisted → lands on `/notifications`. Wrong-password rejection path asserted too. |
| `e2e/tests/notifications.spec.ts` | Inbox page fetches real `GET /notifications` (200 JSON array), then "Mark all read" button fires real `POST /notifications/read-all`. |
| `e2e/tests/alerts.spec.ts` | Admin loads `/alerts` (200 array), clicks Refresh to re-fetch; staff user gets a real 403 from the API on the same path. |
| `e2e/tests/reporting.spec.ts` | Admin visits `/reporting/metrics`, verifies the seeded `on_time_departure_rate` metric is both in the 200 JSON response and visible in the DOM. |

HTML / trace / video artifacts are written to `e2e/playwright-report/`
and are picked up by CI as build artifacts.

### CI pipeline

Because every category runs through `docker compose --profile test`, the
entire pipeline is a single command in any CI system:

```bash
./run_tests.sh all
echo "exit: $?"
```

Exit code is `0` if every category passed, `1` if any category failed,
`2` for setup errors (Docker missing, etc.).

### Test suite layout & coverage notes


```
unit_tests/
  test_dnd_logic.py            DND window logic (boundary, midnight-crossing, critical bypass)
  test_reconciliation_logic.py Discrepancy classification, duplicate detection
  test_signature_logic.py      HMAC-SHA256/512, signed-string construction, timestamp window
  test_alert_severity.py       KPI severity thresholds, deduplication, alert routing/ack, state transitions

API_tests/
  conftest.py                  Session fixtures: test users, tokens, api() helper
  test_auth_api.py             Login, session, logout, reauth
  test_notifications_api.py    Inbox, DND prefs, subscriptions, rules, announce
  test_payments_api.py         Transactions, refunds, imports, callbacks
  test_alerting_api.py         List/filter, stats, acknowledge, close
  test_rbac_api.py             RBAC enforcement across all four roles
  test_ops_api.py              Routes/stops/trips/calendars/configs CRUD + publish/schedule
  test_dispatcher_api.py       Trip lifecycle, conflict mgmt, monitoring dashboard
  test_reconciliation_api.py   Statements, run detail/summary/items
  test_reporting_api.py        Metric detail/compute, schedule CRUD, run list/detail
  test_coverage_gaps.py        Receipt, compensation, audit log detail
  test_reauth_gated.py         ReauthGuard-gated endpoints with strict post-reauth contracts
  test_security.py             HMAC signature, replay window, cross-user ownership, DND

frontend/tests/                (wasm-pack, strict `*.spec.rs` naming)
  component_states.spec.rs     Role-guard routing, rollout state machine, statement import validation
  notification_logic.spec.rs   Notification helpers + StatusFilter contracts
  service_contracts.spec.rs    JSON wire contracts + TOKEN_KEY invariant
  dispatcher_workflows.spec.rs TripConflict filtering + dispatcher RoleGuard
  inbox_panel.spec.rs          NotificationState reducer (acknowledge / dismiss / filter / etc.)
  api_service.spec.rs          Every service-layer request/response shape round-trips
  role_guard.spec.rs           Full role × permission matrix

e2e/tests/                     (Playwright Chromium, no mocks)
  login.spec.ts                Login + token persistence + wrong-password rejection
  notifications.spec.ts        Inbox fetch + mark-all-read POST
  alerts.spec.ts               Admin load/refresh + staff real-403
  reporting.spec.ts            Seeded KPI metric list + DOM visibility
```

#### Static test coverage notes
- DND edge cases (queueing, critical bypass, midnight windows) are covered in unit_tests/test_dnd_logic.py
- Alert routing, deduplication, and acknowledgment transitions are covered in unit_tests/test_alert_severity.py
- Adapter enable/disable logic is statically verifiable via config.rs and environment variables (see above)
- Audit log retention and immutability are enforced at the DB and documented in src/audit/mod.rs and db/migrations/010_audit_extensions.sql

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

### Required path — Docker

The frontend is built and served entirely inside Docker. No host Rust,
wasm-pack, Trunk, or Node installation is required.

```bash
docker compose --profile test up frontend-test
```

This uses `frontend/Dockerfile` to compile the WASM bundle with Trunk inside
a multi-stage image, then serves the result via nginx on
**http://localhost:80**. API XHRs are reverse-proxied to the `api-test`
container — all fetches come from the real backend.

### Frontend tests — Docker

```bash
./run_tests.sh frontend
```

Runs `wasm-pack test --headless --firefox` inside the `frontend-runner`
container.  All files use the `*.spec.rs` naming for strict detectability
and explicitly `use wasm_bindgen_test::*;` as framework evidence.

- `tests/component_states.spec.rs` — auth/role-guard routing, rollout page
  transitions, ops admin CRUD state machines, base64 encoder.
- `tests/notification_logic.spec.rs` — `Notification` helpers, `StatusFilter`
  enum query-param + label contracts.
- `tests/service_contracts.spec.rs` — JSON contract stability between service
  types and the backend (LoginResponse, SessionInfo, Notification, MetricValue),
  plus the `TOKEN_KEY` auth-store ↔ api.rs contract.
- `tests/dispatcher_workflows.spec.rs` — `TripConflict` filtering, dispatcher
  role access, conflict-type categorisation.
- `tests/inbox_panel.spec.rs` — `NotificationState` reducer flow (acknowledge /
  dismiss / mark-all / filter / open-close / error state).
- `tests/api_service.spec.rs` — request/response wire contracts for every
  service layer module (auth, ops, notifications, reporting, alerting) —
  catches field renames on either side of the API.
- `tests/role_guard.spec.rs` — the full role × permission matrix used by
  every `RoleGuard` and `AuthGuard` in the SPA.

### Optional — local hot-reload (NOT part of the required setup)

> **The required setup is Docker-only.**  Nothing in this subsection is
> needed to run the project, tests, or demos — skip it unless you want
> sub-second rebuilds while editing frontend code outside a container.
> These commands are a reference for that niche workflow only; CI and
> reviewers should use the Docker flow above.
>
> See `frontend/Dockerfile` for the canonical build pipeline.

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
