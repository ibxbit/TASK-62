# System Design: TransitOps Backoffice Platform

## 1. Architectural Overview

The system is implemented as a **full-stack monolith** composed of two independently deployable units: a **Rust/Actix-web REST API** and a **Rust/Yew WASM single-page application**. Persistence is handled by **PostgreSQL 16** via **SQLx**, enabling offline-first operation for regional shuttle and bus operators without Internet access.

### Architecture Patterns

- **Domain-Driven Module Decomposition**: Backend is split into 12 focused modules — `auth`, `ops`, `dispatcher`, `notifications`, `payments`, `reconciliation`, `reporting`, `alerting`, `audit`, `crypto`, `scheduler`, and `rbac` — each owning its own routes, handlers, models, and business logic.
- **Event-Bus Architecture**: A persistent event bus (`notifications::bus`) decouples event producers (ops, payments, reconciliation) from consumers (subscribed users). Every event is stored in `notifications.events` before fan-out, guaranteeing at-least-once delivery.
- **Pluggable Channel Adapter Registry**: External notification channels (Email, SMS, WeCom) are registered as trait objects at startup. Each adapter is inert when its connector URL is absent, ensuring the system is fully functional offline using in-app delivery only.
- **Scheduler-as-Service**: A poll-based background scheduler runs five independent jobs at different intervals, each guarded by PostgreSQL advisory locks and `FOR UPDATE SKIP LOCKED` to prevent concurrent execution across nodes.
- **Role-Permission Middleware**: A flat `Permission` enum and handler-level `session.require(Permission::X)` calls enforce fine-grained access control. A `ReauthGuard` extractor adds a 10-minute re-authentication gate on privileged mutations.

---

## 2. Security Design

- **Authentication**: Local username/password only. Passwords are hashed with Argon2/bcrypt. The raw session token (64-hex) is never stored — only its `SHA-256` hash is persisted in `auth.sessions`. Sessions expire after 30 minutes of inactivity; the server revokes and rejects them at the first subsequent request.
- **Re-authentication Gate**: Administrative actions (config publish/unpublish/schedule/rollout, reconciliation runs, metric management, report exports) require `POST /auth/reauth` within the last 10 minutes. Enforced by the `ReauthGuard` extractor in `src/auth/middleware.rs`.
- **Encryption at Rest**: Sensitive fields (card last-4, payer reference, email, statement file content) are encrypted using **AES-256-GCM** with a key loaded from `ENCRYPTION_KEY` (64 hex chars). Each ciphertext blob is self-contained (`nonce[12] || ciphertext+tag[n+16]`). Key rotation is supported via `ENCRYPTION_KEY_PREVIOUS` with automatic fallback during decryption.
- **Data Masking**: Encrypted fields are never returned to the client in plaintext. API responses apply masking functions: `****1234` for card last-4, `j***@example.com` for email, `192.168.1.xxx` for IPv4, and first-two-segment masking for IPv6.
- **Callback HMAC Verification**: Inbound payment gateway webhooks must pass HMAC-SHA256 signature verification over `"<nonce>.<timestamp>.<sha256(body)>"`. Anti-replay is enforced via a 5-minute timestamp window and nonce uniqueness tracked in `payments.callbacks`.
- **Immutable Audit Log**: Every publish/unpublish, reconciliation action, export, and metric change is written as an INSERT-only row in `audit.logs`. Entries are never updated or deleted, with a designed retention period of 7 years.
- **RBAC**: Four roles (`operations_admin`, `dispatcher`, `finance_analyst`, `staff_user`) map to an explicit set of 18+ permissions. Permission checks are the first statement in every HTTP handler.

---

## 3. Core Business Logic Designs

### Operations Configuration Lifecycle

- **State Machine**: Config versions follow a `draft → scheduled | published → archived` lifecycle. Only one version per template can be in `published` status at a time. Publishing a new version atomically archives the previous one inside a database transaction.
- **Scheduled Publish**: Versions in `scheduled` status with `effective_from <= now()` are auto-published by `SystemMaintenanceJob` every 60 seconds using `FOR UPDATE SKIP LOCKED` to prevent double-publish across concurrent nodes.
- **Gradual Rollout by Depot**: Rollout plans (`ops.rollout_plans`) contain ordered stages, each specifying explicit `depot_ids` and a `target_percentage` label. The scheduler activates each stage when its `scheduled_at` time is reached, upserts depot config assignments, and advances the plan's `current_stage` counter.
- **Diff View**: Config diff is computed server-side by comparing the JSON payload of the target version against the most recently published version. The diff endpoint returns structured change objects with `path`, `old`, `new`, and `change_type`.

### Notification Fan-Out and DND

- **Fan-Out Model**: On each 5-second bus tick, up to 50 unprocessed events are fetched and distributed to all subscribers. Delivery receipts (`notifications.deliveries`) are inserted with `ON CONFLICT (event_id, user_id) DO NOTHING`, making fan-out idempotent against bus restarts.
- **DND Enforcement**: Non-critical deliveries for users inside a DND window are inserted as `status = 'queued'`. A `flush_dnd_queue` sweep in the same bus tick promotes queued deliveries to `delivered` for users whose DND window has just ended — using a single bulk UPDATE for efficiency.
- **Critical Bypass**: Deliveries where `effective_severity = 'critical'` skip the DND check entirely and are delivered immediately. The frontend renders critical notifications as a prominent in-app banner.
- **15-Minute Dedup**: Before inserting a delivery, `check_duplicate` queries for any non-dismissed delivery with the same `(user_id, event_type, source_entity_id)` within the last 15 minutes. If found, the delivery is silently suppressed.

### Payment Gateway Abstraction

- **Unified Gateway**: All payment operations go through a gateway abstraction layer. Real gateways are configured via the `payments.gateways` table; the `offline_test` gateway is used for local callback simulation during development and integration testing.
- **Idempotency**: `POST /payments/transactions` and `POST /payments/refunds` both check `idempotency_key` before inserting, returning the existing record with `200 OK` if the key already exists.
- **Compensation Job**: `PaymentCompensationJob` runs every 15 minutes and queries for transactions/refunds/callbacks stuck in transitional statuses, re-queuing them for processing. This handles crashes during callback processing without manual intervention.

### Reconciliation Engine

- **Discrepancy Classification**: After parsing a statement file, each line is matched against `payments.transactions` by reference. Discrepancies are tagged as `missing` (no matching transaction), `amount_mismatch` (delta > $0.01), or `duplicate` (reference appears more than once in the statement).
- **Alert Integration**: A completed reconciliation run with any discrepancies calls `alerting::detector::create_alert(...)`, which inserts an alert row and queues a notification event for subscribers. This routes discrepancy alerts through the same acknowledge/close workflow as KPI anomaly alerts.
- **File Fingerprinting**: The SHA-256 hash of the uploaded file is stored in `payments.statement_imports.file_hash`. Duplicate file submissions (same hash) are rejected before any processing begins.

### KPI Dashboard and Reporting

- **Metric Definitions**: Users with `ReportingMetricsManage` can define custom KPI metrics with a `formula_type` (`custom_sql`, `ratio`, `count`) and optional filters by route, depot, and date range.
- **Scheduled Exports**: Report schedules trigger runs via `SystemMaintenanceJob`. Each run stores results as JSON in `reporting.report_runs`. Exports are generated on demand from stored results, with a watermark injected at export time containing the viewer's username and generation timestamp.
- **Spike Detection**: `KpiAnomalyJob` (30-min interval) compares the latest KPI snapshot against a rolling 7-day average. A deviation beyond the configured threshold triggers `create_alert`, creating an alert and queueing a notification.

---

## 4. Non-Functional Readiness

- **Offline-First**: The system has zero hard dependencies on external network services. All adapters are inert when unconfigured. The entire stack runs from a single `docker compose up`, self-contained with a PostgreSQL instance.
- **Startup Safety**: `ENCRYPTION_KEY` is validated at startup (must be exactly 64 hex chars); a malformed key causes an immediate panic before the HTTP server binds. Database connectivity is verified before any handler is registered.
- **Scheduler Crash Recovery**: `Scheduler::recover_stale_runs()` is called at startup to reset any job-run records left in `running` status from a previous crash, preventing the advisory lock from being permanently held.
- **Observability**: Structured `tracing` with named fields is used throughout. All job runs record start time, end time, outcome (`summary` JSON), and error message in `scheduler.job_runs`. `RUST_LOG=info` produces operational logs; `debug` adds per-event and per-delivery traces.
- **Data Retention**: The `DedupCleanupJob` (1-hour interval) prunes `scheduler.job_runs` older than 7 days, `notifications.channel_deliveries` older than 30 days, and dismissed inbox entries older than 90 days, preventing unbounded table growth.
- **Pagination**: All list endpoints support `?limit=N&offset=M` with limit clamped to 1–100. Queries use `ORDER BY created_at DESC` for predictable pagination.
