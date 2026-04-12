-- ============================================================
-- Migration 007 — Offline Payment Gateway schema additions
-- ============================================================
-- Creates:
--   payments.gateway_configs       per-provider HMAC config
--   payments.compensation_jobs     compensation run audit log
-- Alters:
--   payments.transactions          adds route_id (for reporting drill-down)
--   payments.callbacks             adds callback_timestamp (for anti-replay)
-- Creates view:
--   payments.reconciliation_entries  cents-based alias used by reporting.metrics
-- ============================================================

-- ---- gateway_configs ----
-- Stores per-provider HMAC configuration.
-- hmac_secret is the raw shared secret supplied by the gateway provider.
-- sig_header / nonce_header / ts_header are the request header names used
-- by each provider — differs across Alipay, WeChat Pay, bank APIs, etc.
CREATE TABLE IF NOT EXISTS payments.gateway_configs (
    id             UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    name           VARCHAR(64)  NOT NULL UNIQUE,          -- machine identifier, e.g. 'alipay'
    display_name   VARCHAR(128) NOT NULL,
    hmac_secret    TEXT         NOT NULL,                 -- shared HMAC key (store in Vault / secrets manager in prod)
    hmac_algorithm VARCHAR(16)  NOT NULL DEFAULT 'sha256'
                                CHECK (hmac_algorithm IN ('sha256', 'sha512')),
    sig_header     VARCHAR(64)  NOT NULL DEFAULT 'X-Signature',
    nonce_header   VARCHAR(64)  NOT NULL DEFAULT 'X-Nonce',
    ts_header      VARCHAR(64)  NOT NULL DEFAULT 'X-Timestamp',
    -- Optional: require the timestamp field to also be present in the signed string
    ts_in_sig      BOOLEAN      NOT NULL DEFAULT TRUE,
    is_active      BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- ---- compensation_jobs ----
-- One row per compensation sweep execution for audit and monitoring.
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

-- ---- Extend payments.transactions ----
ALTER TABLE payments.transactions
    ADD COLUMN IF NOT EXISTS route_id UUID REFERENCES ops.routes(id);

-- ---- Extend payments.callbacks ----
-- Stores the timestamp from the callback payload for anti-replay checks.
ALTER TABLE payments.callbacks
    ADD COLUMN IF NOT EXISTS callback_timestamp TIMESTAMPTZ;

-- Index on callback_timestamp for the >5-min rejection query.
CREATE INDEX IF NOT EXISTS idx_callbacks_ts
    ON payments.callbacks (callback_timestamp);

-- ---- payments.reconciliation_entries (view) ----
-- Cents-based projection of reconciliation_items used by the reporting metrics engine.
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

-- Seed: one offline/test gateway for development.
-- IMPORTANT: seeded as INACTIVE (is_active = FALSE) by default.
-- Before activating any gateway, replace hmac_secret with a strong,
-- randomly-generated value (min 32 characters, not a placeholder).
INSERT INTO payments.gateway_configs
    (name, display_name, hmac_secret, sig_header, nonce_header, ts_header, is_active)
VALUES
    ('offline_test', 'Offline Test Gateway',
     'CHANGE_ME_IN_PRODUCTION',
     'X-Signature', 'X-Nonce', 'X-Timestamp',
     FALSE)
ON CONFLICT (name) DO UPDATE SET is_active = FALSE;
