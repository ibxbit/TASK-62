-- ============================================================
-- Migration 009 — Anomaly alerting schema
-- ============================================================
-- Creates the `alerting` schema with one table:
--   alerting.alerts  — lifecycle-tracked anomaly alerts with
--                      severity levels and ack/close workflow.
--
-- Alert sources:
--   reconciliation_mismatch — fired after each reconciliation run
--                             that contains discrepancies
--   kpi_anomaly             — fired by the background KPI checker
--                             when a metric deviates from baseline
--
-- Status flow:
--   open → acknowledged → closed
--   open →               closed      (direct close without ack)
--
-- Deduplication: at most one *open* alert per (alert_type, source_entity_id).
-- A new alert can be raised once the previous one is closed.
-- ============================================================

CREATE SCHEMA IF NOT EXISTS alerting;

CREATE TABLE IF NOT EXISTS alerting.alerts (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Classification
    alert_type       TEXT        NOT NULL
                                 CHECK (alert_type IN (
                                     'reconciliation_mismatch',
                                     'kpi_anomaly'
                                 )),
    severity         TEXT        NOT NULL
                                 CHECK (severity IN ('info', 'warning', 'critical')),
    status           TEXT        NOT NULL DEFAULT 'open'
                                 CHECK (status IN ('open', 'acknowledged', 'closed')),

    -- Source tracing
    source_domain    TEXT        NOT NULL,     -- 'payments' | 'reporting'
    source_entity_id UUID,                     -- run_id (reconciliation) or metric_definition_id (KPI)

    -- Human-readable content
    title            TEXT        NOT NULL,
    description      TEXT,
    payload          JSONB       NOT NULL DEFAULT '{}',

    -- Acknowledge workflow
    acknowledged_by  UUID        REFERENCES auth.users(id),
    acknowledged_at  TIMESTAMPTZ,

    -- Close workflow
    closed_by        UUID        REFERENCES auth.users(id),
    closed_at        TIMESTAMPTZ,
    close_reason     TEXT,

    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Enforce at-most-one open alert per source entity + type.
-- Partial unique index: only applies when status = 'open' and entity is known.
CREATE UNIQUE INDEX IF NOT EXISTS idx_alerts_open_dedup
    ON alerting.alerts (alert_type, source_entity_id)
    WHERE status = 'open' AND source_entity_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_alerts_status
    ON alerting.alerts (status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_alerts_severity
    ON alerting.alerts (severity, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_alerts_type
    ON alerting.alerts (alert_type, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_alerts_entity
    ON alerting.alerts (source_entity_id)
    WHERE source_entity_id IS NOT NULL;
