# TransitOps Backoffice Platform — REST API Specification
**Version:** 1.0  **Base URL:** `http://localhost:8080`  **Auth:** `Authorization: Bearer <64-hex-token>`

---

## Conventions

| Rule | Detail |
|---|---|
| Auth | All endpoints except `POST /auth/login` require `Authorization: Bearer <token>` |
| Reauth | Endpoints marked ⚠️ additionally require `POST /auth/reauth` within the last 10 min |
| Pagination | `?limit=50&offset=0` (limit clamped 1–100) |
| Timestamps | ISO 8601 UTC strings, e.g. `"2026-04-15T00:01:00Z"` |
| IDs | UUID v4 |
| Errors | `{ "error": "<message>", "code": "<SNAKE_CASE>" }` with appropriate HTTP status |

### Error Codes

| HTTP | `code` | Meaning |
|---|---|---|
| 400 | `BAD_REQUEST` | Invalid input / business rule violation |
| 401 | `UNAUTHORIZED` | Missing / expired / invalid token |
| 403 | `FORBIDDEN` | Insufficient permission or reauth required |
| 404 | `NOT_FOUND` | Resource does not exist |
| 409 | `CONFLICT` | Unique constraint violation |
| 500 | `INTERNAL` | Server-side error |

---

## Authentication (`/auth`)

### `POST /auth/login`
Authenticate with username and password. Returns a 64-hex session token.

**No auth required.**

**Request:**
```json
{ "username": "admin", "password": "AdminPass123!" }
```

**Response `200 OK`:**
```json
{
  "token": "a1b2c3...64hex",
  "username": "admin",
  "role": "operations_admin",
  "session_id": "uuid",
  "expires_at": "2026-04-10T20:00:00Z"
}
```

**Errors:** `401` (bad credentials), `400` (missing fields)

---

### `GET /auth/session`
Returns the current session context.

**Response `200 OK`:**
```json
{
  "session_id": "uuid",
  "username": "admin",
  "role": "operations_admin",
  "last_activity_at": "2026-04-10T18:00:00Z",
  "last_reauth_at": "2026-04-10T17:55:00Z"
}
```

---

### `POST /auth/reauth`
Re-authenticates the current session. Required before privileged admin/finance actions.

**Request:**
```json
{ "password": "AdminPass123!" }
```

**Response `200 OK`:**
```json
{ "message": "Re-authentication successful", "reauthed_at": "2026-04-10T18:05:00Z" }
```

---

### `POST /auth/logout`
Revokes the current session token.

**Response `200 OK`:**
```json
{ "message": "Logged out" }
```

---

## Operations — Config Lifecycle (`/ops/configs`)

### `GET /ops/configs`
List all config templates. **Roles:** All.

**Response `200 OK`:**
```json
[
  {
    "id": "uuid",
    "name": "Spring Schedule 2026",
    "domain": "route_operations",
    "current_version_id": "uuid",
    "current_status": "published",
    "created_at": "2026-01-01T00:00:00Z"
  }
]
```

---

### `POST /ops/configs`
Create a new config template. **Roles:** `operations_admin`.

**Request:**
```json
{ "name": "Spring Schedule 2026", "domain": "route_operations", "description": "Q2 schedule" }
```

**Response `201 Created`**: config template object.

---

### `GET /ops/configs/{template_id}/versions`
List all versions for a template. **Roles:** All.

---

### `POST /ops/configs/{template_id}/versions`
Create a new draft version. **Roles:** `operations_admin`.

**Request:**
```json
{ "notes": "Added Route 42 afternoon variant", "payload": { "routes": [...] } }
```

---

### `GET /ops/configs/{template_id}/versions/{version_id}/diff`
Returns a structured diff between this version and the previously published version. **Roles:** `operations_admin`, `dispatcher`.

**Response `200 OK`:**
```json
{
  "version_id": "uuid",
  "base_version_id": "uuid",
  "changes": [
    { "path": "routes[2].departure_time", "old": "08:00", "new": "08:15", "change_type": "modified" }
  ]
}
```

---

### `POST /ops/configs/{template_id}/versions/{version_id}/publish` ⚠️
Immediately publish this version (archives current published version). **Roles:** `operations_admin`.

**Response `200 OK`:** updated version object.

---

### `POST /ops/configs/{template_id}/versions/{version_id}/unpublish` ⚠️
Unpublish the current published version (returns to draft). **Roles:** `operations_admin`.

---

### `POST /ops/configs/{template_id}/versions/{version_id}/schedule` ⚠️
Schedule this version to auto-publish at a future time. **Roles:** `operations_admin`.

**Request:**
```json
{ "effective_from": "2026-04-15T00:01:00Z" }
```

---

### `POST /ops/configs/{template_id}/versions/{version_id}/rollout` ⚠️
Create a gradual rollout plan (by depot). **Roles:** `operations_admin`.

**Request:**
```json
{
  "stages": [
    { "target_percentage": 10, "depot_ids": ["uuid-depot-a"], "scheduled_at": "2026-04-15T06:00:00Z" },
    { "target_percentage": 50, "depot_ids": ["uuid-depot-a", "uuid-depot-b"], "scheduled_at": "2026-04-17T06:00:00Z" },
    { "target_percentage": 100, "depot_ids": ["ALL"], "scheduled_at": "2026-04-22T06:00:00Z" }
  ]
}
```

---

## Operations — Routes (`/ops/routes`)

### `GET /ops/routes`
List all routes. **Roles:** All.

**Query params:** `?status=active&limit=50&offset=0`

**Response `200 OK`:**
```json
[
  {
    "id": "uuid",
    "route_code": "R42",
    "name": "City Centre — Airport",
    "description": "Express shuttle",
    "status": "active",
    "created_at": "2026-01-01T00:00:00Z"
  }
]
```

---

### `POST /ops/routes`
Create a route. **Roles:** `operations_admin`, `dispatcher`.

**Request:**
```json
{ "route_code": "R42", "name": "City Centre — Airport", "description": "Express shuttle" }
```

---

### `GET /ops/routes/{id}`
Get a single route. **Roles:** All.

---

### `PUT /ops/routes/{id}`
Update a route. **Roles:** `operations_admin`.

---

### `DELETE /ops/routes/{id}`
Delete a route. **Roles:** `operations_admin`.

**Response `204 No Content`.**

---

## Operations — Stops (`/ops/routes/{route_id}/stops`)

### `GET /ops/routes/{route_id}/stops`
List stops for a route. **Roles:** All.

**Response `200 OK`:**
```json
[
  { "id": "uuid", "route_id": "uuid", "name": "Terminal 1", "sequence": 1, "lat": 31.23, "lng": 121.47 }
]
```

### `POST /ops/routes/{route_id}/stops`
Add a stop. **Roles:** `operations_admin`.

### `PUT /ops/routes/{route_id}/stops/{stop_id}`
Update a stop. **Roles:** `operations_admin`.

### `DELETE /ops/routes/{route_id}/stops/{stop_id}`
Remove a stop. **Roles:** `operations_admin`.

---

## Operations — Calendars (`/ops/calendars`)

### `GET /ops/calendars` — List calendars. **Roles:** All.
### `POST /ops/calendars` — Create calendar. **Roles:** `operations_admin`.
### `GET /ops/calendars/{id}` — Get calendar. **Roles:** All.
### `PUT /ops/calendars/{id}` — Update calendar. **Roles:** `operations_admin`.
### `DELETE /ops/calendars/{id}` — Delete calendar. **Roles:** `operations_admin`.

**Calendar object:**
```json
{
  "id": "uuid",
  "name": "Weekday Schedule",
  "effective_from": "2026-04-01",
  "effective_to": "2026-06-30",
  "days_of_week": [1, 2, 3, 4, 5],
  "exception_dates": ["2026-05-01"]
}
```

---

## Operations — Fare Rules (`/ops/fare-rules`)

### `GET /ops/fare-rules` — List. **Roles:** All.
### `POST /ops/fare-rules` — Create. **Roles:** `operations_admin`.
### `PUT /ops/fare-rules/{id}` — Update. **Roles:** `operations_admin`.
### `DELETE /ops/fare-rules/{id}` — Delete. **Roles:** `operations_admin`.

---

## Dispatcher (`/dispatcher`)

### `GET /dispatcher/trips`
List trips. **Roles:** `operations_admin`, `dispatcher`.

**Query params:** `?route_id=uuid&date=2026-04-15&status=active`

### `POST /dispatcher/trips`
Create trip. **Roles:** `dispatcher`.

### `PUT /dispatcher/trips/{id}`
Adjust trip (time, vehicle, driver). **Roles:** `dispatcher`.

### `GET /dispatcher/conflicts`
List detected scheduling conflicts. **Roles:** `operations_admin`, `dispatcher`.

**Response `200 OK`:**
```json
[
  {
    "id": "uuid",
    "conflict_type": "overlap",
    "trip_ids": ["uuid-a", "uuid-b"],
    "description": "Trips A and B share a vehicle at the same time",
    "detected_at": "2026-04-10T08:00:00Z",
    "status": "open"
  }
]
```

---

## Notifications (`/notifications`)

### `GET /notifications`
In-app inbox for the authenticated user.

**Query params:** `?status=unread&limit=20&offset=0`

**Response `200 OK`:**
```json
[
  {
    "id": "uuid",
    "event_type": "alerts.anomaly.kpi_deviation",
    "title": "On-time rate dropped below threshold",
    "severity": "warning",
    "status": "unread",
    "created_at": "2026-04-10T10:00:00Z"
  }
]
```

---

### `POST /notifications/{delivery_id}/read`
Mark a delivery as read.

### `POST /notifications/{delivery_id}/dismiss`
Dismiss a delivery.

---

### `GET /notifications/preferences`
Get DND preferences for the authenticated user.

**Response `200 OK`:**
```json
{
  "dnd_enabled": true,
  "dnd_start": "22:00",
  "dnd_end": "07:00"
}
```

### `PUT /notifications/preferences`
Update DND preferences.

**Request:**
```json
{ "dnd_enabled": true, "dnd_start": "22:00", "dnd_end": "07:00" }
```

---

### `GET /notifications/subscriptions`
List event subscriptions for the authenticated user.

### `POST /notifications/subscriptions`
Subscribe to an event type.

**Request:**
```json
{ "event_type": "ops.conflict.detected", "channels": ["in_app", "email"] }
```

### `DELETE /notifications/subscriptions/{id}`
Unsubscribe.

---

### `GET /notifications/rules`
List alert subscription rules for the authenticated user.

### `POST /notifications/rules`
Create a rule.

**Request:**
```json
{
  "rule_name": "High refund rate alert",
  "rule_type": "threshold",
  "config": {
    "metric_key": "refund_rate",
    "operator": ">",
    "threshold": 0.05
  }
}
```

**`rule_type` values:** `keyword`, `topic`, `threshold`, `spike`

### `GET /notifications/rules/{id}` — Get rule (own only).
### `PUT /notifications/rules/{id}` — Update rule (own only).
### `DELETE /notifications/rules/{id}` — Delete rule (own only).

---

### `POST /notifications/announce`
Broadcast an announcement to all users (or targeted roles). **Roles:** `operations_admin`.

**Request:**
```json
{
  "title": "System Maintenance",
  "message": "The system will be unavailable from 02:00–03:00.",
  "severity": "info",
  "target_roles": ["all"]
}
```

---

## Payments (`/payments`)

### `GET /payments/transactions`
List transactions. **Roles:** `finance_analyst`, `operations_admin`.

**Query params:** `?status=pending&trip_id=uuid&limit=50&offset=0`

### `POST /payments/transactions`
Create transaction (idempotent via `idempotency_key`). **Roles:** `finance_analyst`.

**Request:**
```json
{
  "idempotency_key": "trip-42-rider-007-20260410",
  "amount": "25.50",
  "currency": "CNY",
  "payment_method": "card",
  "trip_id": "uuid",
  "route_id": "uuid",
  "card_last4": "1234",
  "payer_ref": "ACCT-9876"
}
```

**`payment_method` values:** `cash`, `card`, `mobile`, `bank_transfer`, `voucher`, `other`

**Response `201 Created`:**
```json
{
  "id": "uuid",
  "idempotency_key": "...",
  "amount": 25.50,
  "currency": "CNY",
  "payment_method": "card",
  "status": "pending",
  "card_last4": "****1234",
  "has_payer_ref": true,
  "created_at": "2026-04-10T09:00:00Z"
}
```

### `GET /payments/transactions/{id}` — Get single transaction.

---

### `POST /payments/callbacks/{gateway}`
Receive an inbound payment gateway webhook. **No session auth** — verified via HMAC signature.

**Required headers:**
- `X-Signature`: HMAC-SHA256 of `"<nonce>.<timestamp>.<sha256(body)>"` using gateway secret
- `X-Nonce`: unique string (UUID recommended)
- `X-Timestamp`: Unix seconds; rejected if `now - timestamp > 300`

**Request body (JSON):**
```json
{
  "transaction_ref": "trip-42-rider-007-20260410",
  "status": "COMPLETED",
  "amount": "25.50"
}
```

**Response `200 OK`:**
```json
{ "callback_id": "uuid", "status": "received" }
```

**Errors:** `400` (missing headers / bad timestamp), `401` (bad signature / nonce reused)

---

### `POST /payments/callbacks/simulate`
Simulate a gateway callback (bypasses signature verification). **Roles:** `finance_analyst`. Requires `PaymentsTransactionsWrite`.

**Request:**
```json
{
  "transaction_id": "uuid",
  "gateway": "offline_test",
  "status": "completed",
  "amount_cents": 2550
}
```

---

### `GET /payments/imports` — List statement imports. **Roles:** `finance_analyst`.
### `POST /payments/imports` — Upload a statement file.

**Request:**
```json
{
  "filename": "statement_20260410.csv",
  "format": "csv",
  "source": "removable_media",
  "content_base64": "aWQsYW1vdW50..."
}
```

### `POST /payments/imports/{id}/process` — Parse and match against transactions.

---

### `GET /payments/refunds` — List refunds. **Roles:** `finance_analyst`, `operations_admin`.
### `POST /payments/refunds` — Request refund. **Roles:** `finance_analyst`.

**Request:**
```json
{
  "transaction_id": "uuid",
  "idempotency_key": "refund-txn42-20260410",
  "amount": "25.50",
  "reason": "Duplicate charge"
}
```

### `GET /payments/refunds/{id}` — Get refund.
### `POST /payments/refunds/{id}/approve` — Approve. **Roles:** `operations_admin`.
### `POST /payments/refunds/{id}/process` — Process (complete). **Roles:** `operations_admin`.

---

## Reconciliation (`/reconciliation`)

### `GET /reconciliation/runs`
List reconciliation runs. **Roles:** `finance_analyst`, `operations_admin`.

**Response `200 OK`:**
```json
[
  {
    "id": "uuid",
    "statement_import_id": "uuid",
    "run_date": "2026-04-10",
    "status": "completed",
    "total_checked": 1502,
    "matched": 1490,
    "mismatches": 8,
    "missing": 3,
    "duplicates": 1,
    "run_by": "uuid",
    "created_at": "2026-04-10T09:00:00Z"
  }
]
```

### `POST /reconciliation/runs` ⚠️
Start a reconciliation run. **Roles:** `finance_analyst`.

**Request:**
```json
{ "statement_import_id": "uuid", "run_date": "2026-04-10" }
```

### `GET /reconciliation/runs/{id}` — Get run details.
### `GET /reconciliation/runs/{id}/discrepancies` — List tagged discrepancies.

**Discrepancy object:**
```json
{
  "id": "uuid",
  "run_id": "uuid",
  "transaction_id": "uuid",
  "discrepancy_type": "amount_mismatch",
  "expected_amount": 25.50,
  "actual_amount": 25.00,
  "delta": 0.50,
  "notes": "Exceeds $0.01 tolerance"
}
```

**`discrepancy_type` values:** `missing`, `amount_mismatch`, `duplicate`

---

## Reporting (`/reporting`)

### `GET /reporting/metrics`
List KPI metric definitions. **Roles:** All.

### `POST /reporting/metrics` ⚠️
Create a metric. **Roles:** `finance_analyst`, `operations_admin`.

**Request:**
```json
{
  "metric_key": "on_time_departure_rate",
  "display_name": "On-Time Departure Rate",
  "formula_type": "custom_sql",
  "formula": "SELECT COUNT(*) FILTER (WHERE departure_delta_minutes <= 2)::float / COUNT(*) FROM trip_events",
  "filters": { "route_id": null, "depot_id": null }
}
```

### `PUT /reporting/metrics/{id}` ⚠️ — Update metric.
### `DELETE /reporting/metrics/{id}` ⚠️ — Delete metric.

---

### `GET /reporting/schedules` — List report schedules. **Roles:** All.
### `POST /reporting/schedules` ⚠️ — Create schedule. **Roles:** `finance_analyst`, `operations_admin`.

**Request:**
```json
{
  "name": "Daily Operations Summary",
  "metric_ids": ["uuid-metric-a", "uuid-metric-b"],
  "schedule": "daily",
  "schedule_time": "06:00",
  "recipients": ["uuid-user-a"]
}
```

### `PUT /reporting/schedules/{id}` ⚠️ — Update schedule.
### `DELETE /reporting/schedules/{id}` ⚠️ — Delete schedule.
### `POST /reporting/schedules/{id}/trigger` ⚠️ — Manually trigger a report run.

---

### `GET /reporting/runs` — List report run history. **Roles:** All.
### `GET /reporting/runs/{id}` — Get a single run.

### `GET /reporting/runs/{id}/export` ⚠️
Export run as PDF or CSV with automatic watermark (viewer username + timestamp).

**Query params:** `?format=pdf|csv`

**Response:** Binary file download (`Content-Disposition: attachment`).

---

## Alerting (`/alerts`)

### `GET /alerts`
List alerts. **Roles:** `operations_admin`, `dispatcher`, `finance_analyst`.

**Query params:** `?status=open&severity=warning&limit=50&offset=0`

**Response `200 OK`:**
```json
[
  {
    "id": "uuid",
    "alert_type": "kpi_anomaly",
    "severity": "warning",
    "source_domain": "reporting",
    "source_entity_id": "uuid",
    "title": "On-time rate dropped 15% vs 7-day average",
    "status": "open",
    "created_at": "2026-04-10T10:00:00Z",
    "acknowledged_at": null,
    "closed_at": null
  }
]
```

### `GET /alerts/{id}` — Get single alert.

### `POST /alerts/{id}/acknowledge`
Acknowledge alert. **Roles:** `operations_admin`, `finance_analyst`.

**Request:** `{}` (empty body accepted)

### `POST /alerts/{id}/close`
Close alert. **Roles:** `operations_admin`, `finance_analyst`.

### `GET /alerts/stats`
Alert summary counts by status and severity. **Roles:** `operations_admin`, `dispatcher`, `finance_analyst`.

---

### `GET /alerts/rules`
List alert subscription rules for the authenticated user. Also see `/notifications/rules`.

---

## Audit Log (`/audit`)

### `GET /audit/logs`
Read immutable audit log. **Roles:** `operations_admin` only.

**Query params:** `?action=publish&actor_id=uuid&from=2026-01-01&to=2026-04-10&limit=50&offset=0`

**Response `200 OK`:**
```json
[
  {
    "id": "uuid",
    "actor_id": "uuid",
    "actor_username": "admin",
    "action": "config.publish",
    "resource_type": "config_version",
    "resource_id": "uuid",
    "ip_address": "192.168.1.xxx",
    "metadata": { "template_id": "uuid" },
    "created_at": "2026-04-10T09:05:00Z"
  }
]
```

**Retention:** 7 years. Log entries are immutable (INSERT only, no UPDATE/DELETE).

---

## Background Jobs (Internal — Not Exposed as HTTP)

| Job | Interval | Purpose |
|---|---|---|
| `NotificationBusJob` | 5 s | Event fan-out, DND flush, keyword/spike rule evaluation |
| `PaymentCompensationJob` | 15 min | Retry stuck transactions, refunds, unprocessed callbacks |
| `SystemMaintenanceJob` | 60 s | Auto-publish scheduled configs, activate rollout stages, fire scheduled reports |
| `KpiAnomalyJob` | 30 min | Compare KPI snapshots to rolling average, raise alerts |
| `DedupCleanupJob` | 1 h | Prune old job runs, channel deliveries, dismissed inbox entries |

---

## Permissions Matrix

| Permission | operations_admin | dispatcher | finance_analyst | staff_user |
|---|:---:|:---:|:---:|:---:|
| OpsRoutesRead | ✅ | ✅ | ✅ | ✅ |
| OpsRoutesWrite | ✅ | ✅ | — | — |
| OpsRoutesDelete | ✅ | — | — | — |
| OpsConfigRead | ✅ | ✅ | — | — |
| OpsConfigPublish | ✅ | — | — | — |
| PaymentsTransactionsRead | ✅ | — | ✅ | — |
| PaymentsTransactionsWrite | — | — | ✅ | — |
| PaymentsRefundsRead | ✅ | — | ✅ | — |
| PaymentsRefundsWrite | — | — | ✅ | — |
| PaymentsRefundsApprove | ✅ | — | — | — |
| PaymentsStatementsImport | — | — | ✅ | — |
| PaymentsReconciliationRead | ✅ | — | ✅ | — |
| PaymentsReconciliationRun | — | — | ✅ | — |
| AlertsRead | ✅ | ✅ | ✅ | — |
| AlertsManage | ✅ | — | ✅ | — |
| ReportingRead | ✅ | ✅ | ✅ | ✅ |
| ReportingMetricsManage | ✅ | — | ✅ | — |
| AuditLogsRead | ✅ | — | — | — |
| NotificationsAnnounce | ✅ | — | — | — |
