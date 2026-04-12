-- =============================================================================
-- TransitOps Backoffice Platform — PostgreSQL Schema
-- =============================================================================
-- Tables are created in strict dependency order so this file can be applied
-- to a clean database without errors.
-- Migrations (db/migrations/*.sql) extend this base schema with additional
-- columns, indexes, and tables.
-- =============================================================================

-- ---------------------------------------------------------------------------
-- Schemas
-- ---------------------------------------------------------------------------
CREATE SCHEMA IF NOT EXISTS auth;
CREATE SCHEMA IF NOT EXISTS ops;
CREATE SCHEMA IF NOT EXISTS notifications;
CREATE SCHEMA IF NOT EXISTS payments;
CREATE SCHEMA IF NOT EXISTS reporting;
CREATE SCHEMA IF NOT EXISTS audit;
CREATE SCHEMA IF NOT EXISTS scheduler;

-- Ensure the transitops_app role exists (idempotent)
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = 'transitops_app') THEN
        CREATE ROLE transitops_app LOGIN PASSWORD 'transitops_secret';
    END IF;
END$$;


-- =============================================================================
-- SCHEMA: auth
-- =============================================================================

-- ---------------------------------------------------------------------------
-- auth.roles
-- ---------------------------------------------------------------------------
CREATE TABLE auth.roles (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name        VARCHAR(64) NOT NULL UNIQUE,   -- operations_admin | dispatcher | finance_analyst | staff_user
    description TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- auth.permissions
-- ---------------------------------------------------------------------------
CREATE TABLE auth.permissions (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name        VARCHAR(128) NOT NULL UNIQUE,  -- e.g. ops:trips:write
    domain      VARCHAR(64)  NOT NULL,         -- ops | payments | notifications | reporting | audit
    action      VARCHAR(32)  NOT NULL,         -- read | write | delete | admin
    description TEXT,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- auth.users
-- Sensitive PII (email, full_name) stored encrypted via application layer.
-- ---------------------------------------------------------------------------
CREATE TABLE auth.users (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    username            VARCHAR(64) NOT NULL UNIQUE,
    password_hash       TEXT        NOT NULL,
    role_id             UUID        NOT NULL REFERENCES auth.roles(id),
    -- email stored encrypted (pgp_sym_encrypt); BYTEA
    email_encrypted     BYTEA       NOT NULL,
    is_active           BOOLEAN     NOT NULL DEFAULT TRUE,
    last_login_at       TIMESTAMPTZ,
    password_changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at          TIMESTAMPTZ,            -- soft delete
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_users_role_id   ON auth.users(role_id);
CREATE INDEX idx_users_is_active ON auth.users(is_active) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- auth.sessions
-- ---------------------------------------------------------------------------
CREATE TABLE auth.sessions (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID        NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    token_hash   TEXT        NOT NULL UNIQUE,  -- SHA-256 of the raw JWT
    issued_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,
    revoked_at   TIMESTAMPTZ,
    ip_address   INET,
    user_agent   TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT sessions_not_expired CHECK (expires_at > issued_at)
);

CREATE INDEX idx_sessions_user_id    ON auth.sessions(user_id);
CREATE INDEX idx_sessions_expires_at ON auth.sessions(expires_at) WHERE revoked_at IS NULL;

-- ---------------------------------------------------------------------------
-- auth.role_permissions  (M:N junction)
-- ---------------------------------------------------------------------------
CREATE TABLE auth.role_permissions (
    role_id       UUID NOT NULL REFERENCES auth.roles(id)       ON DELETE CASCADE,
    permission_id UUID NOT NULL REFERENCES auth.permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (role_id, permission_id)
);


-- =============================================================================
-- SCHEMA: ops
-- =============================================================================

-- ---------------------------------------------------------------------------
-- ops.routes
-- ---------------------------------------------------------------------------
CREATE TABLE ops.routes (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    code        VARCHAR(32) NOT NULL UNIQUE,
    name        VARCHAR(128) NOT NULL,
    description TEXT,
    status      VARCHAR(16)  NOT NULL DEFAULT 'draft'
                             CHECK (status IN ('draft','active','inactive')),
    depot_id    UUID,
    deleted_at  TIMESTAMPTZ,
    created_by  UUID        NOT NULL REFERENCES auth.users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_routes_status ON ops.routes(status) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- ops.stops
-- ---------------------------------------------------------------------------
CREATE TABLE ops.stops (
    id             UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    route_id       UUID         NOT NULL REFERENCES ops.routes(id) ON DELETE RESTRICT,
    code           VARCHAR(32)  NOT NULL,
    name           VARCHAR(128) NOT NULL,
    sequence_order SMALLINT     NOT NULL CHECK (sequence_order >= 0),
    latitude       NUMERIC(10,7),
    longitude      NUMERIC(10,7),
    deleted_at     TIMESTAMPTZ,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    UNIQUE (route_id, sequence_order),
    UNIQUE (route_id, code)
);

CREATE INDEX idx_stops_route_id ON ops.stops(route_id) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- ops.config_templates
-- ---------------------------------------------------------------------------
CREATE TABLE ops.config_templates (
    id           UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    key          VARCHAR(128) NOT NULL UNIQUE,  -- e.g. 'fare_rules', 'schedule_policy'
    domain       VARCHAR(64)  NOT NULL,
    description  TEXT,
    json_schema  JSONB,                          -- JSON Schema for payload validation
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- ops.config_versions
-- ---------------------------------------------------------------------------
CREATE TABLE ops.config_versions (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id     UUID        NOT NULL REFERENCES ops.config_templates(id) ON DELETE RESTRICT,
    version_number  INT         NOT NULL CHECK (version_number > 0),
    status          VARCHAR(16) NOT NULL DEFAULT 'draft'
                                CHECK (status IN ('draft','published','scheduled','archived')),
    payload         JSONB       NOT NULL,
    effective_from  TIMESTAMPTZ,
    effective_to    TIMESTAMPTZ,
    published_at    TIMESTAMPTZ,
    published_by    UUID        REFERENCES auth.users(id),
    scheduled_at    TIMESTAMPTZ,
    created_by      UUID        NOT NULL REFERENCES auth.users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (template_id, version_number),
    CONSTRAINT config_versions_effective_range
        CHECK (effective_to IS NULL OR effective_to > effective_from)
);

CREATE UNIQUE INDEX idx_config_one_published
    ON ops.config_versions(template_id)
    WHERE status = 'published';

CREATE INDEX idx_config_versions_template ON ops.config_versions(template_id, status);

-- ---------------------------------------------------------------------------
-- ops.trips
-- ---------------------------------------------------------------------------
CREATE TABLE ops.trips (
    id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    route_id             UUID        NOT NULL REFERENCES ops.routes(id) ON DELETE RESTRICT,
    trip_code            VARCHAR(64) NOT NULL UNIQUE,
    scheduled_departure  TIMESTAMPTZ NOT NULL,
    scheduled_arrival    TIMESTAMPTZ NOT NULL,
    actual_departure     TIMESTAMPTZ,
    actual_arrival       TIMESTAMPTZ,
    status               VARCHAR(16) NOT NULL DEFAULT 'scheduled'
                                     CHECK (status IN ('scheduled','in_progress','completed','cancelled')),
    assigned_driver_id   UUID        REFERENCES auth.users(id),
    deleted_at           TIMESTAMPTZ,
    created_by           UUID        NOT NULL REFERENCES auth.users(id),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT trips_arrival_after_departure
        CHECK (scheduled_arrival > scheduled_departure)
);

CREATE INDEX idx_trips_route_id            ON ops.trips(route_id);
CREATE INDEX idx_trips_status              ON ops.trips(status) WHERE deleted_at IS NULL;
CREATE INDEX idx_trips_scheduled_departure ON ops.trips(scheduled_departure);
CREATE INDEX idx_trips_assigned_driver     ON ops.trips(assigned_driver_id) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- ops.rollout_stages
-- ---------------------------------------------------------------------------
CREATE TABLE ops.rollout_stages (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plan_id         UUID NOT NULL,
    stage_number    INT NOT NULL,
    depot_ids       UUID[] NOT NULL DEFAULT '{}',
    status          VARCHAR(32) NOT NULL DEFAULT 'pending',
    scheduled_at    TIMESTAMPTZ,
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);


-- =============================================================================
-- SCHEMA: notifications
-- =============================================================================

-- ---------------------------------------------------------------------------
-- notifications.event_definitions
-- ---------------------------------------------------------------------------
CREATE TABLE notifications.event_definitions (
    id             UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type     VARCHAR(128) NOT NULL UNIQUE,  -- e.g. 'ops.trip.created'
    domain         VARCHAR(64)  NOT NULL,
    description    TEXT,
    payload_schema JSONB,                          -- expected payload shape
    severity       VARCHAR(16)  NOT NULL DEFAULT 'info'
                                CHECK (severity IN ('info','warning','critical')),
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- notifications.events
-- ---------------------------------------------------------------------------
CREATE TABLE notifications.events (
    id               UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type       VARCHAR(128) NOT NULL
                         REFERENCES notifications.event_definitions(event_type)
                         ON UPDATE CASCADE ON DELETE RESTRICT,
    source_domain    VARCHAR(64)  NOT NULL,
    source_entity_id UUID,
    actor_id         UUID,                   -- user who triggered; nullable for system events
    payload          JSONB        NOT NULL DEFAULT '{}',
    severity         VARCHAR(16)  NOT NULL DEFAULT 'info'
                                  CHECK (severity IN ('info','warning','critical')),
    processed_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX idx_events_event_type  ON notifications.events(event_type);
CREATE INDEX idx_events_created_at  ON notifications.events(created_at DESC);
CREATE INDEX idx_events_entity      ON notifications.events(source_domain, source_entity_id);
CREATE INDEX idx_events_pending     ON notifications.events(created_at ASC) WHERE processed_at IS NULL;

-- ---------------------------------------------------------------------------
-- notifications.dnd_settings
-- ---------------------------------------------------------------------------
CREATE TABLE notifications.dnd_settings (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id           UUID        NOT NULL UNIQUE REFERENCES auth.users(id) ON DELETE CASCADE,
    is_enabled        BOOLEAN     NOT NULL DEFAULT FALSE,
    start_time        TIME,
    end_time          TIME,
    days_of_week      SMALLINT[]  DEFAULT '{}'::SMALLINT[],
    channels_blocked  VARCHAR(16)[] DEFAULT '{}'::VARCHAR[],
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT dnd_valid_time_range
        CHECK (start_time IS NULL OR end_time IS NULL OR end_time > start_time)
);


-- =============================================================================
-- SCHEMA: payments
-- =============================================================================

-- ---------------------------------------------------------------------------
-- payments.transactions
-- ---------------------------------------------------------------------------
CREATE TABLE payments.transactions (
    id                      UUID           PRIMARY KEY DEFAULT gen_random_uuid(),
    idempotency_key         VARCHAR(128)   NOT NULL UNIQUE,
    trip_id                 UUID           REFERENCES ops.trips(id) ON DELETE RESTRICT,
    amount                  NUMERIC(14,2)  NOT NULL CHECK (amount >= 0),
    currency                CHAR(3)        NOT NULL DEFAULT 'CNY',
    payment_method          VARCHAR(32)    NOT NULL
                                           CHECK (payment_method IN ('cash','card','mobile','bank_transfer','voucher','other')),
    status                  VARCHAR(20)    NOT NULL DEFAULT 'pending'
                                           CHECK (status IN ('pending','completed','failed','refunded','partially_refunded','voided')),
    collected_by            UUID           REFERENCES auth.users(id),
    created_at              TIMESTAMPTZ    NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ    NOT NULL DEFAULT now()
);

CREATE INDEX idx_txn_trip_id      ON payments.transactions(trip_id);
CREATE INDEX idx_txn_status       ON payments.transactions(status);
CREATE INDEX idx_txn_created_at   ON payments.transactions(created_at DESC);
CREATE INDEX idx_txn_collected_by ON payments.transactions(collected_by);

-- ---------------------------------------------------------------------------
-- payments.callbacks
-- ---------------------------------------------------------------------------
CREATE TABLE payments.callbacks (
    id                 UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    transaction_id     UUID        REFERENCES payments.transactions(id) ON DELETE SET NULL,
    nonce              VARCHAR(256) NOT NULL UNIQUE,   -- replay-prevention
    signature          TEXT        NOT NULL,           -- HMAC or provider signature
    payload_hash       TEXT        NOT NULL,           -- SHA-256 of raw payload
    source             VARCHAR(64) NOT NULL,           -- provider name
    received_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at       TIMESTAMPTZ,
    callback_timestamp TIMESTAMPTZ,
    status             VARCHAR(16) NOT NULL DEFAULT 'received'
                                   CHECK (status IN ('received','processed','invalid','replayed'))
);

CREATE INDEX idx_callbacks_transaction ON payments.callbacks(transaction_id);
CREATE INDEX idx_callbacks_received_at ON payments.callbacks(received_at DESC);
CREATE INDEX idx_callbacks_status      ON payments.callbacks(status);
CREATE INDEX idx_callbacks_ts          ON payments.callbacks(callback_timestamp);

-- ---------------------------------------------------------------------------
-- payments.refunds
-- ---------------------------------------------------------------------------
CREATE TABLE payments.refunds (
    id               UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    transaction_id   UUID          NOT NULL REFERENCES payments.transactions(id) ON DELETE RESTRICT,
    idempotency_key  VARCHAR(128)  NOT NULL UNIQUE,
    amount           NUMERIC(14,2) NOT NULL CHECK (amount > 0),
    reason           TEXT,
    status           VARCHAR(16)   NOT NULL DEFAULT 'pending'
                                   CHECK (status IN ('pending','approved','processing','completed','rejected')),
    requested_by     UUID          NOT NULL REFERENCES auth.users(id),
    approved_by      UUID          REFERENCES auth.users(id),
    processed_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ   NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ   NOT NULL DEFAULT now()
);

CREATE INDEX idx_refunds_transaction ON payments.refunds(transaction_id);
CREATE INDEX idx_refunds_status      ON payments.refunds(status);

-- ---------------------------------------------------------------------------
-- payments.reconciliation_runs
-- ---------------------------------------------------------------------------
CREATE TABLE payments.reconciliation_runs (
    id                  UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    run_date            DATE          NOT NULL UNIQUE,
    status              VARCHAR(16)   NOT NULL DEFAULT 'pending'
                                      CHECK (status IN ('pending','running','completed','failed')),
    total_expected      NUMERIC(16,2) NOT NULL DEFAULT 0,
    total_collected     NUMERIC(16,2) NOT NULL DEFAULT 0,
    total_discrepancy   NUMERIC(16,2) GENERATED ALWAYS AS (total_collected - total_expected) STORED,
    discrepancy_count   INT           NOT NULL DEFAULT 0,
    started_at          TIMESTAMPTZ,
    completed_at        TIMESTAMPTZ,
    run_by              UUID          REFERENCES auth.users(id),
    notes               TEXT,
    created_at          TIMESTAMPTZ   NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ   NOT NULL DEFAULT now()
);

CREATE INDEX idx_recon_run_date ON payments.reconciliation_runs(run_date DESC);
CREATE INDEX idx_recon_status   ON payments.reconciliation_runs(status);

-- ---------------------------------------------------------------------------
-- payments.reconciliation_items
-- ---------------------------------------------------------------------------
CREATE TABLE payments.reconciliation_items (
    id                UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id            UUID          NOT NULL REFERENCES payments.reconciliation_runs(id) ON DELETE CASCADE,
    transaction_id    UUID          REFERENCES payments.transactions(id) ON DELETE SET NULL,
    expected_amount   NUMERIC(14,2) NOT NULL,
    actual_amount     NUMERIC(14,2) NOT NULL,
    discrepancy       NUMERIC(14,2) GENERATED ALWAYS AS (actual_amount - expected_amount) STORED,
    match_status      VARCHAR(16)   NOT NULL
                                    CHECK (match_status IN ('matched','discrepancy','missing','extra')),
    notes             TEXT,
    created_at        TIMESTAMPTZ   NOT NULL DEFAULT now(),
    UNIQUE (run_id, transaction_id)
);

CREATE INDEX idx_recon_items_run    ON payments.reconciliation_items(run_id);
CREATE INDEX idx_recon_items_status ON payments.reconciliation_items(match_status);

-- ---------------------------------------------------------------------------
-- payments.statement_imports
-- ---------------------------------------------------------------------------
CREATE TABLE payments.statement_imports (
    id                    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    filename              VARCHAR(256) NOT NULL,
    file_hash             VARCHAR(64)  NOT NULL UNIQUE,  -- SHA-256 of original file
    source                VARCHAR(128) NOT NULL,          -- bank / provider name
    import_date           DATE         NOT NULL,
    status                VARCHAR(16)  NOT NULL DEFAULT 'pending'
                                       CHECK (status IN ('pending','processing','completed','failed')),
    total_records         INT          NOT NULL DEFAULT 0,
    processed_records     INT          NOT NULL DEFAULT 0,
    error_count           INT          NOT NULL DEFAULT 0,
    imported_by           UUID         NOT NULL REFERENCES auth.users(id),
    raw_content_encrypted BYTEA,                          -- pgp_sym_encrypt of file bytes
    created_at            TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX idx_stmt_import_date   ON payments.statement_imports(import_date DESC);
CREATE INDEX idx_stmt_import_status ON payments.statement_imports(status);

-- ---------------------------------------------------------------------------
-- payments.statement_import_lines
-- ---------------------------------------------------------------------------
CREATE TABLE payments.statement_import_lines (
    id                     UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    import_id              UUID          NOT NULL REFERENCES payments.statement_imports(id) ON DELETE CASCADE,
    line_number            INT           NOT NULL CHECK (line_number > 0),
    transaction_ref        VARCHAR(128),
    amount                 NUMERIC(14,2) NOT NULL,
    transaction_date       DATE          NOT NULL,
    description            TEXT,
    matched_transaction_id UUID          REFERENCES payments.transactions(id) ON DELETE SET NULL,
    match_status           VARCHAR(16)   NOT NULL DEFAULT 'unmatched'
                                         CHECK (match_status IN ('matched','unmatched','ambiguous','excluded')),
    created_at             TIMESTAMPTZ   NOT NULL DEFAULT now(),
    UNIQUE (import_id, line_number)
);

CREATE INDEX idx_stmt_lines_import      ON payments.statement_import_lines(import_id);
CREATE INDEX idx_stmt_lines_match       ON payments.statement_import_lines(match_status);
CREATE INDEX idx_stmt_lines_matched_txn ON payments.statement_import_lines(matched_transaction_id)
    WHERE matched_transaction_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- payments.compensation_jobs
-- (base definition; migration 007 also has CREATE TABLE IF NOT EXISTS)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS payments.compensation_jobs (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    job_type        TEXT        NOT NULL
                                CHECK (job_type IN ('stuck_transactions', 'pending_refunds', 'callback_retry')),
    status          TEXT        NOT NULL DEFAULT 'running'
                                CHECK (status IN ('running', 'completed', 'failed')),
    affected_count  INT         NOT NULL DEFAULT 0,
    error_message   TEXT,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_comp_jobs_type   ON payments.compensation_jobs (job_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_comp_jobs_status ON payments.compensation_jobs (status);

-- ---------------------------------------------------------------------------
-- payments.gateway_configs
-- (base definition; migration 007 also has CREATE TABLE IF NOT EXISTS)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS payments.gateway_configs (
    id             UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    name           VARCHAR(64)  NOT NULL UNIQUE,
    display_name   VARCHAR(128) NOT NULL,
    hmac_secret    TEXT         NOT NULL,
    hmac_algorithm VARCHAR(16)  NOT NULL DEFAULT 'sha256'
                                CHECK (hmac_algorithm IN ('sha256', 'sha512')),
    sig_header     VARCHAR(64)  NOT NULL DEFAULT 'X-Signature',
    nonce_header   VARCHAR(64)  NOT NULL DEFAULT 'X-Nonce',
    ts_header      VARCHAR(64)  NOT NULL DEFAULT 'X-Timestamp',
    ts_in_sig      BOOLEAN      NOT NULL DEFAULT TRUE,
    is_active      BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- payments.reconciliation_entries (view)
-- Depends on: reconciliation_items, reconciliation_runs, transactions
-- Must appear AFTER those tables.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE VIEW payments.reconciliation_entries AS
SELECT
    ri.id,
    ri.transaction_id,
    ROUND(ri.expected_amount * 100)::BIGINT AS expected_amount_cents,
    ROUND(ri.actual_amount   * 100)::BIGINT AS settled_amount_cents,
    rr.run_date::TIMESTAMPTZ               AS reconciled_at,
    tx.route_id
FROM payments.reconciliation_items ri
JOIN payments.reconciliation_runs rr ON rr.id = ri.run_id
LEFT JOIN payments.transactions  tx  ON tx.id = ri.transaction_id;


-- =============================================================================
-- SCHEMA: reporting
-- =============================================================================

-- ---------------------------------------------------------------------------
-- reporting.metric_definitions
-- ---------------------------------------------------------------------------
CREATE TABLE reporting.metric_definitions (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    metric_key          TEXT        NOT NULL UNIQUE,
    display_name        TEXT        NOT NULL,
    description         TEXT,
    formula_type        TEXT        NOT NULL
        CHECK (formula_type IN ('on_time_departure_rate', 'refund_rate', 'reconciliation_mismatch_count', 'custom_sql')),
    dimension_keys      TEXT[]      NOT NULL DEFAULT '{}',
    config              JSONB       NOT NULL DEFAULT '{}',
    is_builtin          BOOLEAN     NOT NULL DEFAULT FALSE,
    is_active           BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- reporting.kpi_results
-- ---------------------------------------------------------------------------
CREATE TABLE reporting.kpi_results (
    id           UUID           PRIMARY KEY DEFAULT gen_random_uuid(),
    metric_id    UUID           NOT NULL REFERENCES reporting.metric_definitions(id) ON DELETE CASCADE,
    period_type  VARCHAR(16)    NOT NULL
                                CHECK (period_type IN ('daily','weekly','monthly','quarterly','annual')),
    period_start DATE           NOT NULL,
    period_end   DATE           NOT NULL,
    dimensions   JSONB          NOT NULL DEFAULT '{}',
    value        NUMERIC(24,6)  NOT NULL,
    computed_at  TIMESTAMPTZ    NOT NULL DEFAULT now(),
    created_at   TIMESTAMPTZ    NOT NULL DEFAULT now(),
    CONSTRAINT kpi_period_range CHECK (period_end >= period_start),
    UNIQUE (metric_id, period_type, period_start, dimensions)
);

CREATE INDEX idx_kpi_metric_period ON reporting.kpi_results(metric_id, period_type, period_start DESC);
CREATE INDEX idx_kpi_dimensions    ON reporting.kpi_results USING GIN (dimensions);
CREATE INDEX idx_kpi_computed_at   ON reporting.kpi_results(computed_at DESC);

-- ---------------------------------------------------------------------------
-- reporting.report_snapshots
-- ---------------------------------------------------------------------------
CREATE TABLE reporting.report_snapshots (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    report_type   VARCHAR(64) NOT NULL,
    generated_by  UUID        NOT NULL REFERENCES auth.users(id),
    parameters    JSONB       NOT NULL DEFAULT '{}',
    result_data   JSONB       NOT NULL DEFAULT '{}',
    generated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_snapshots_type       ON reporting.report_snapshots(report_type);
CREATE INDEX idx_snapshots_expires_at ON reporting.report_snapshots(expires_at);

-- ---------------------------------------------------------------------------
-- reporting.report_runs  (used by the reporting handlers)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS reporting.report_runs (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    report_type  VARCHAR(64) NOT NULL,
    generated_by UUID        NOT NULL REFERENCES auth.users(id),
    parameters   JSONB       NOT NULL DEFAULT '{}',
    result_data  JSONB       NOT NULL DEFAULT '{}',
    value        NUMERIC(24,6),
    generated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);


-- =============================================================================
-- SCHEMA: audit
-- Immutable append-only log. No UPDATE, no DELETE ever permitted.
-- Partitioned by year to support 7-year retention + efficient pruning.
-- =============================================================================

CREATE TABLE audit.audit_logs (
    id               UUID        NOT NULL DEFAULT gen_random_uuid(),
    actor_id         UUID,
    actor_username   VARCHAR(64),
    actor_role       VARCHAR(64),
    action           VARCHAR(64) NOT NULL,
    domain           VARCHAR(64) NOT NULL,
    entity_type      VARCHAR(64) NOT NULL,
    entity_id        TEXT        NOT NULL,
    before_state     JSONB,
    after_state      JSONB,
    diff             JSONB,
    ip_address       INET,
    user_agent       TEXT,
    session_id       UUID,
    metadata         JSONB       NOT NULL DEFAULT '{}',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    retention_until  DATE        NOT NULL,
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

CREATE TABLE audit.audit_logs_2024 PARTITION OF audit.audit_logs
    FOR VALUES FROM ('2024-01-01 00:00:00+00') TO ('2025-01-01 00:00:00+00');

CREATE TABLE audit.audit_logs_2025 PARTITION OF audit.audit_logs
    FOR VALUES FROM ('2025-01-01 00:00:00+00') TO ('2026-01-01 00:00:00+00');

CREATE TABLE audit.audit_logs_2026 PARTITION OF audit.audit_logs
    FOR VALUES FROM ('2026-01-01 00:00:00+00') TO ('2027-01-01 00:00:00+00');

CREATE TABLE audit.audit_logs_2027 PARTITION OF audit.audit_logs
    FOR VALUES FROM ('2027-01-01 00:00:00+00') TO ('2028-01-01 00:00:00+00');

CREATE TABLE audit.audit_logs_2028 PARTITION OF audit.audit_logs
    FOR VALUES FROM ('2028-01-01 00:00:00+00') TO ('2029-01-01 00:00:00+00');

CREATE TABLE audit.audit_logs_2029 PARTITION OF audit.audit_logs
    FOR VALUES FROM ('2029-01-01 00:00:00+00') TO ('2030-01-01 00:00:00+00');

CREATE TABLE audit.audit_logs_2030 PARTITION OF audit.audit_logs
    FOR VALUES FROM ('2030-01-01 00:00:00+00') TO ('2031-01-01 00:00:00+00');

CREATE INDEX idx_audit_created_at ON audit.audit_logs(created_at DESC);
CREATE INDEX idx_audit_actor_id   ON audit.audit_logs(actor_id) WHERE actor_id IS NOT NULL;
CREATE INDEX idx_audit_entity     ON audit.audit_logs(domain, entity_type, entity_id);
CREATE INDEX idx_audit_action     ON audit.audit_logs(action);
CREATE INDEX idx_audit_session    ON audit.audit_logs(session_id) WHERE session_id IS NOT NULL;


-- =============================================================================
-- SCHEMA: scheduler
-- (also created by migration 013 with CREATE TABLE — safe due to IF NOT EXISTS)
-- =============================================================================

CREATE TABLE IF NOT EXISTS scheduler.job_runs (
    id           UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    job_name     TEXT         NOT NULL,
    status       TEXT         NOT NULL DEFAULT 'running'
                              CHECK (status IN ('running', 'success', 'failed', 'skipped')),
    started_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    finished_at  TIMESTAMPTZ,
    duration_ms  INTEGER,
    outcome      JSONB,
    error_msg    TEXT
);

CREATE INDEX IF NOT EXISTS idx_job_runs_name_time
    ON scheduler.job_runs (job_name, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_job_runs_running
    ON scheduler.job_runs (started_at)
    WHERE status = 'running';


-- =============================================================================
-- SECURITY: Application DB Role (least-privilege)
-- =============================================================================

GRANT USAGE ON SCHEMA auth, ops, notifications, payments, reporting, audit, scheduler
    TO transitops_app;

GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA auth          TO transitops_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA ops           TO transitops_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA notifications TO transitops_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA payments      TO transitops_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA reporting     TO transitops_app;
GRANT SELECT, INSERT ON audit.audit_logs                                   TO transitops_app;
GRANT SELECT, INSERT, UPDATE ON scheduler.job_runs                         TO transitops_app;


-- =============================================================================
-- UTILITY: updated_at auto-maintenance trigger
-- =============================================================================

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

DO $$
DECLARE
    tbl RECORD;
BEGIN
    FOR tbl IN
        SELECT table_schema, table_name
        FROM information_schema.columns
        WHERE column_name = 'updated_at'
          AND table_schema IN ('auth','ops','notifications','payments','reporting','scheduler')
    LOOP
        EXECUTE format(
            'CREATE TRIGGER trg_set_updated_at
             BEFORE UPDATE ON %I.%I
             FOR EACH ROW EXECUTE FUNCTION set_updated_at()',
            tbl.table_schema, tbl.table_name
        );
    END LOOP;
END;
$$;
