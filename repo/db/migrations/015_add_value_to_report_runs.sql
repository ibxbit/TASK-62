-- Migration: Add value column to reporting.report_runs
ALTER TABLE reporting.report_runs ADD COLUMN value NUMERIC(24,6);
