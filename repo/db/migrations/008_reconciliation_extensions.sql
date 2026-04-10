-- ============================================================
-- Migration 008 — Reconciliation system extensions
-- ============================================================
-- Extends the existing payments reconciliation tables with:
--   • discrepancy_type column on reconciliation_items
--   • statement_import_id link on reconciliation_runs
--   • duplicate_of_line_id on statement_import_lines
--   • format_errors JSONB on statement_imports for validation audit
--   • fingerprint_expected on statement_imports for pre-import verification
-- ============================================================

-- ---- reconciliation_items: discrepancy type tag ----
-- Classifies the nature of each discrepancy beyond the existing match_status.
ALTER TABLE payments.reconciliation_items
    ADD COLUMN IF NOT EXISTS discrepancy_type TEXT
        CHECK (discrepancy_type IN (
            'matched',
            'amount_mismatch',
            'missing_from_statement',
            'extra_in_statement',
            'duplicate'
        ));

-- ---- reconciliation_runs: link to the source statement import ----
ALTER TABLE payments.reconciliation_runs
    ADD COLUMN IF NOT EXISTS statement_import_id UUID
        REFERENCES payments.statement_imports(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_recon_runs_import
    ON payments.reconciliation_runs (statement_import_id);

-- ---- statement_imports: format / fingerprint metadata ----
-- format_errors: array of validation error messages (empty = valid)
ALTER TABLE payments.statement_imports
    ADD COLUMN IF NOT EXISTS format_errors       JSONB     NOT NULL DEFAULT '[]';

-- fingerprint_expected: SHA-256 provided by the operator at upload time.
-- If present, the system verifies content hash matches before accepting the file.
ALTER TABLE payments.statement_imports
    ADD COLUMN IF NOT EXISTS fingerprint_expected TEXT;

-- ---- statement_import_lines: duplicate pointer ----
-- When a line is identified as a duplicate of another, this column points to
-- the canonical (first-occurrence) line for the same reference.
ALTER TABLE payments.statement_import_lines
    ADD COLUMN IF NOT EXISTS duplicate_of_line_id UUID
        REFERENCES payments.statement_import_lines(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_stmt_lines_duplicate
    ON payments.statement_import_lines (duplicate_of_line_id)
    WHERE duplicate_of_line_id IS NOT NULL;
