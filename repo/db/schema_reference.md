# TransitOps — Schema Reference

## Schema Layout

```
postgres
├── auth          users, roles, permissions, role_permissions, sessions
├── ops           routes, stops, trips, config_templates, config_versions
├── notifications event_definitions, events, subscriptions, deliveries,
│                 inbox_messages, dnd_settings
├── payments      transactions, callbacks, refunds, reconciliation_runs,
│                 reconciliation_items, statement_imports, statement_import_lines
├── reporting     metric_definitions, kpi_results, report_snapshots
└── audit         audit_logs (partitioned by year, 2024–2030)
```

---

## Table Relationships

```
auth.roles ──< auth.role_permissions >── auth.permissions
auth.roles ──< auth.users
auth.users ──< auth.sessions

auth.users ──< ops.routes           (created_by)
auth.users ──< ops.trips            (created_by, assigned_driver_id)
auth.users ──< ops.config_versions  (created_by, published_by)
ops.routes ──< ops.stops
ops.routes ──< ops.trips
ops.config_templates ──< ops.config_versions

notifications.event_definitions ──< notifications.events
notifications.event_definitions ──< notifications.subscriptions
auth.users ──< notifications.subscriptions
notifications.events ──< notifications.deliveries
auth.users  ──< notifications.deliveries
notifications.deliveries ──< notifications.inbox_messages
auth.users ──1 notifications.dnd_settings

ops.trips ──< payments.transactions
auth.users ──< payments.transactions  (collected_by)
payments.transactions ──< payments.callbacks
payments.transactions ──< payments.refunds
payments.reconciliation_runs ──< payments.reconciliation_items
payments.transactions ──< payments.reconciliation_items
payments.statement_imports ──< payments.statement_import_lines
payments.transactions ──< payments.statement_import_lines  (matched_transaction_id)

reporting.metric_definitions ──< reporting.kpi_results
auth.users ──< reporting.report_snapshots

-- audit.audit_logs has NO FK relationships (by design — immutability + retention)
```

---

## Key Constraints

| Table | Constraint | Purpose |
|---|---|---|
| `auth.users` | `UNIQUE username` | Login identity |
| `auth.sessions` | `UNIQUE token_hash` | One token = one session |
| `ops.routes` | `UNIQUE code` | Route deduplication |
| `ops.trips` | `UNIQUE trip_code` | Operational reference key |
| `ops.stops` | `UNIQUE (route_id, sequence_order)` | No duplicate positions |
| `ops.config_versions` | `UNIQUE (template_id, version_number)` | Version numbering |
| `ops.config_versions` | `UNIQUE INDEX WHERE status='published'` | One live config per template |
| `notifications.subscriptions` | `UNIQUE (user_id, event_type, channel)` | No duplicate sub |
| `notifications.deliveries` | `UNIQUE (event_id + delivery_id via inbox)` | Via inbox FK |
| `payments.transactions` | `UNIQUE idempotency_key` | Prevents duplicate submission |
| `payments.callbacks` | `UNIQUE nonce` | Replay-attack prevention |
| `payments.refunds` | `UNIQUE idempotency_key` | Prevents duplicate refund |
| `payments.statement_imports` | `UNIQUE file_hash` | Prevents duplicate file |
| `reporting.kpi_results` | `UNIQUE (metric_id, period_type, period_start, dimensions)` | No duplicate KPI entry |

---

## Soft Delete Strategy

| Table | Soft Delete Column | Hard Delete? |
|---|---|---|
| `auth.users` | `deleted_at TIMESTAMPTZ` | Never |
| `ops.routes` | `deleted_at TIMESTAMPTZ` | Never |
| `ops.stops` | `deleted_at TIMESTAMPTZ` | Never |
| `ops.trips` | `deleted_at TIMESTAMPTZ` | Never |
| `payments.transactions` | — | Never (financial record) |
| `audit.audit_logs` | — | Never (append-only, retention_until enforced externally) |
| All others | — | Standard CASCADE or RESTRICT |

---

## Encryption Strategy

| Field | Method | Storage Type |
|---|---|---|
| `auth.users.email_encrypted` | `pgp_sym_encrypt(value, app_key)` | `BYTEA` |
| `auth.users.full_name_encrypted` | `pgp_sym_encrypt(value, app_key)` | `BYTEA` |
| `payments.transactions.card_last4_encrypted` | `pgp_sym_encrypt(value, app_key)` | `BYTEA` |
| `payments.transactions.payer_ref_encrypted` | `pgp_sym_encrypt(value, app_key)` | `BYTEA` |
| `payments.statement_imports.raw_content_encrypted` | `pgp_sym_encrypt(bytes, app_key)` | `BYTEA` |

Decryption example:
```sql
SELECT pgp_sym_decrypt(email_encrypted, current_setting('app.encryption_key'))
FROM auth.users WHERE id = $1;
```

Encryption key is passed as a session-scoped setting (`SET LOCAL app.encryption_key = '...'`)
and never stored in the database.

---

## Audit Log Retention

- Partitioned by `RANGE(created_at)`, one partition per calendar year.
- `retention_until` computed column = `created_at + 7 years`.
- To purge expired data: `DROP TABLE audit.audit_logs_YYYY` — O(1), no row scan.
- New yearly partition must be created each January (via cron or `pg_partman`).
- DB role `transitops_app` has `INSERT + SELECT` only — no `UPDATE`/`DELETE`.

---

## Index Summary

| Schema | Index | Columns | Type |
|---|---|---|---|
| auth | `idx_users_role_id` | `role_id` | btree |
| auth | `idx_sessions_expires_at` | `expires_at` WHERE not revoked | btree |
| ops | `idx_trips_scheduled_departure` | `scheduled_departure` | btree |
| ops | `idx_config_one_published` | `template_id` WHERE published | unique partial |
| notifications | `idx_events_created_at` | `created_at DESC` | btree |
| notifications | `idx_events_entity` | `(source_domain, source_entity_id)` | btree |
| notifications | `idx_inbox_user_unread` | `(user_id, is_read)` WHERE unread | partial btree |
| payments | `idx_txn_created_at` | `created_at DESC` | btree |
| payments | `idx_callbacks_status` | `status` | btree |
| reporting | `idx_kpi_metric_period` | `(metric_id, period_type, period_start DESC)` | btree |
| reporting | `idx_kpi_dimensions` | `dimensions` | GIN |
| audit | `idx_audit_entity` | `(domain, entity_type, entity_id)` | btree |
| audit | `idx_audit_actor_id` | `actor_id` WHERE not null | partial btree |
