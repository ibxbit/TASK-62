-- Migration: Add missing amount and value columns for Rust/SQLx compatibility

-- Add amount to payments.gateway_configs if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_schema = 'payments' AND table_name = 'gateway_configs' AND column_name = 'amount'
    ) THEN
        ALTER TABLE payments.gateway_configs ADD COLUMN amount NUMERIC(14,2) NOT NULL DEFAULT 0;
    END IF;
END$$;

-- Add amount to payments.statement_imports if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_schema = 'payments' AND table_name = 'statement_imports' AND column_name = 'amount'
    ) THEN
        ALTER TABLE payments.statement_imports ADD COLUMN amount NUMERIC(14,2) NOT NULL DEFAULT 0;
    END IF;
END$$;

-- Add value to reporting.report_runs if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_schema = 'reporting' AND table_name = 'report_runs' AND column_name = 'value'
    ) THEN
        ALTER TABLE reporting.report_runs ADD COLUMN value NUMERIC(24,6);
    END IF;
END$$;
