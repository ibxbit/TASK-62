-- ============================================================
-- Migration 006 — KPI / Reporting schema
-- ============================================================
-- Creates the `reporting` schema with four tables:
--   metric_definitions  — registry of named metrics (built-in + custom)
--   metric_snapshots    — time-series aggregation cache
--   scheduled_reports   — recurring report configuration
--   report_runs         — per-run execution records with JSONB result
--
-- Also adds `depot_id` to `ops.routes` for the depot drill-down dimension.
-- ============================================================

-- ---------- schema ----------
CREATE SCHEMA IF NOT EXISTS reporting;

-- ---------- depot dimension on routes ----------
ALTER TABLE ops.routes
    ADD COLUMN IF NOT EXISTS depot_id UUID REFERENCES ops.routes(id);

-- ============================================================
-- reporting.metric_definitions
-- ============================================================
-- metric_key is the stable programmatic identifier used in code.
-- formula_type drives which aggregation function the engine calls.
-- dimension_keys controls which drill-down dimensions are valid.
-- tolerance_minutes is used only by on_time_departure_rate.
-- ============================================================
CREATE TABLE IF NOT EXISTS reporting.metric_definitions (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    metric_key          TEXT        NOT NULL UNIQUE,
    display_name        TEXT        NOT NULL,
    description         TEXT,
    formula_type        TEXT        NOT NULL
        CHECK (formula_type IN ('on_time_departure_rate', 'refund_rate', 'reconciliation_mismatch_count', 'custom_sql')),
    dimension_keys      TEXT[]      NOT NULL DEFAULT '{}',
    -- extra config (e.g. tolerance_minutes for on-time calculations)
    config              JSONB       NOT NULL DEFAULT '{}',
    is_builtin          BOOLEAN     NOT NULL DEFAULT FALSE,
    is_active           BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ============================================================
-- reporting.metric_snapshots
-- ============================================================
-- Pre-computed aggregation results keyed by (metric, granularity, period, dimensions).
-- Avoids re-computing expensive queries on every export.
-- ============================================================
CREATE TABLE IF NOT EXISTS reporting.metric_snapshots (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    metric_id       UUID        NOT NULL REFERENCES reporting.metric_definitions(id) ON DELETE CASCADE,
    granularity     TEXT        NOT NULL CHECK (granularity IN ('hour', 'day', 'week', 'month')),
    period_start    TIMESTAMPTZ NOT NULL,
    period_end      TIMESTAMPTZ NOT NULL,
    -- dimension filters applied when this snapshot was computed
    route_id        UUID,
    depot_id        UUID,
    -- the computed scalar value
    value           NUMERIC     NOT NULL,
    sample_count    BIGINT      NOT NULL DEFAULT 0,
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (metric_id, granularity, period_start, route_id, depot_id)
);

CREATE INDEX IF NOT EXISTS idx_metric_snapshots_metric_period
    ON reporting.metric_snapshots (metric_id, period_start DESC);

-- ============================================================
-- reporting.scheduled_reports
-- ============================================================
CREATE TABLE IF NOT EXISTS reporting.scheduled_reports (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT        NOT NULL,
    metric_ids      UUID[]      NOT NULL,
    -- schedule: 'daily' | 'weekly' | 'monthly'
    schedule        TEXT        NOT NULL CHECK (schedule IN ('daily', 'weekly', 'monthly')),
    -- filter dimensions (NULL = no filter)
    route_id        UUID,
    depot_id        UUID,
    date_range_days INT         NOT NULL DEFAULT 30,   -- rolling window for each run
    granularity     TEXT        NOT NULL DEFAULT 'day'
        CHECK (granularity IN ('hour', 'day', 'week', 'month')),
    output_format   TEXT        NOT NULL DEFAULT 'csv'
        CHECK (output_format IN ('csv', 'pdf')),
    recipient_user_ids UUID[]   NOT NULL DEFAULT '{}',
    is_active       BOOLEAN     NOT NULL DEFAULT TRUE,
    next_run_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_run_at     TIMESTAMPTZ,
    created_by      UUID        NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_scheduled_reports_next_run
    ON reporting.scheduled_reports (next_run_at)
    WHERE is_active = TRUE;

-- ============================================================
-- reporting.report_runs
-- ============================================================
CREATE TABLE IF NOT EXISTS reporting.report_runs (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    scheduled_id    UUID        REFERENCES reporting.scheduled_reports(id) ON DELETE SET NULL,
    -- ad-hoc runs leave scheduled_id NULL and carry trigger_user_id
    trigger_user_id UUID,
    metric_ids      UUID[]      NOT NULL,
    route_id        UUID,
    depot_id        UUID,
    date_from       TIMESTAMPTZ NOT NULL,
    date_to         TIMESTAMPTZ NOT NULL,
    granularity     TEXT        NOT NULL DEFAULT 'day',
    output_format   TEXT        NOT NULL DEFAULT 'csv',
    status          TEXT        NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    -- JSONB payload: { "rows": [...], "meta": {...} }
    result_data     JSONB,
    error_message   TEXT,
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_report_runs_scheduled ON reporting.report_runs (scheduled_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_report_runs_status    ON reporting.report_runs (status) WHERE status = 'pending';
