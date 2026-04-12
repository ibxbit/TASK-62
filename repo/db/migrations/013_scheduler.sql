-- =============================================================================
-- Migration 013: Background job scheduler
--
-- Creates scheduler.job_runs for execution history and outcome tracking.
-- Distributed mutual exclusion is handled via pg_advisory_lock (in-memory,
-- session-scoped) so no lock table is needed.
-- =============================================================================

CREATE SCHEMA IF NOT EXISTS scheduler;

-- ---------------------------------------------------------------------------
-- Execution history
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS scheduler.job_runs (
    id           UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    job_name     TEXT         NOT NULL,
    -- 'running'  — lock acquired, execution in progress
    -- 'success'  — completed without error
    -- 'failed'   — run() returned Err
    -- 'skipped'  — advisory lock was held by another instance
    status       TEXT         NOT NULL DEFAULT 'running'
                              CHECK (status IN ('running', 'success', 'failed', 'skipped')),
    started_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    finished_at  TIMESTAMPTZ,
    duration_ms  INTEGER,                    -- set on completion
    outcome      JSONB,                      -- job-specific summary (success only)
    error_msg    TEXT                        -- error description (failed only)
);

-- per-job history, most-recent-first
CREATE INDEX IF NOT EXISTS idx_job_runs_name_time
    ON scheduler.job_runs (job_name, started_at DESC);

-- find stale running records on restart
CREATE INDEX IF NOT EXISTS idx_job_runs_running
    ON scheduler.job_runs (started_at)
    WHERE status = 'running';

COMMENT ON TABLE scheduler.job_runs IS
    'Append-only execution log written by the in-process scheduler. '
    'Rows with status=running and started_at older than 1 hour are '
    'artefacts of a crashed process and are repaired to status=failed '
    'during the next startup (Scheduler::recover_stale_runs).';

GRANT SELECT, INSERT, UPDATE ON scheduler.job_runs TO transitops_app;
