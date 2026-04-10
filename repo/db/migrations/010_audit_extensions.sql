-- ============================================================
-- Migration 010 — Audit log partition extensions
-- ============================================================
-- The audit.audit_logs table and 2024–2030 partitions are
-- defined in schema.sql.  This migration extends coverage to
-- 2033 and adds an additional covering index for the most
-- common auditor query pattern (date + action + domain).
--
-- Yearly partitions must be added each January going forward.
-- Run with pg_cron or manually before the partition boundary.
-- ============================================================

-- ---- Extend yearly partitions ----
CREATE TABLE IF NOT EXISTS audit.audit_logs_2031
    PARTITION OF audit.audit_logs
    FOR VALUES FROM ('2031-01-01 00:00:00+00') TO ('2032-01-01 00:00:00+00');

CREATE TABLE IF NOT EXISTS audit.audit_logs_2032
    PARTITION OF audit.audit_logs
    FOR VALUES FROM ('2032-01-01 00:00:00+00') TO ('2033-01-01 00:00:00+00');

CREATE TABLE IF NOT EXISTS audit.audit_logs_2033
    PARTITION OF audit.audit_logs
    FOR VALUES FROM ('2033-01-01 00:00:00+00') TO ('2034-01-01 00:00:00+00');

-- ---- Covering index for auditor dashboard queries ----
-- Supports:  SELECT ... WHERE domain = $1 AND action = $2 AND created_at >= $3
-- Useful for compliance exports filtered by domain + event type + date window.
CREATE INDEX IF NOT EXISTS idx_audit_domain_action_created
    ON audit.audit_logs (domain, action, created_at DESC);

-- ---- Retention purge helper view ----
-- Lists entries whose 7-year retention window has expired.
-- A scheduled job (pg_cron, external script) can DELETE based on this view.
-- The view itself does nothing; it is a guard rail to prevent accidental
-- early deletion of in-window entries.
CREATE OR REPLACE VIEW audit.expired_logs AS
SELECT id, created_at, retention_until
FROM   audit.audit_logs
WHERE  retention_until < CURRENT_DATE;

COMMENT ON VIEW audit.expired_logs IS
    'Audit entries past their 7-year retention_until date. '
    'Safe to purge via: DELETE FROM audit.audit_logs WHERE id IN (SELECT id FROM audit.expired_logs).';
