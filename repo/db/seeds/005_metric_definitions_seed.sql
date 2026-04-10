-- ============================================================
-- Seed 005 — Built-in KPI metric definitions
-- ============================================================
-- Idempotent: uses INSERT ... ON CONFLICT DO NOTHING so re-running
-- this seed never overwrites admin-edited rows.
-- ============================================================

INSERT INTO reporting.metric_definitions
    (metric_key, display_name, description, formula_type, dimension_keys, config, is_builtin)
VALUES
    (
        'on_time_departure_rate',
        'On-Time Departure Rate',
        'Percentage of trips that departed within the configured tolerance window of their scheduled departure time.',
        'on_time_departure_rate',
        ARRAY['route_id', 'depot_id'],
        '{"tolerance_minutes": 5}'::jsonb,
        TRUE
    ),
    (
        'refund_rate',
        'Refund Rate',
        'Percentage of completed payment transactions that resulted in a refund within the reporting period.',
        'refund_rate',
        ARRAY['route_id', 'depot_id'],
        '{}'::jsonb,
        TRUE
    ),
    (
        'reconciliation_mismatch_count',
        'Reconciliation Mismatch Count',
        'Number of payment transactions where the settled amount differs from the expected amount by more than the configured threshold.',
        'reconciliation_mismatch_count',
        ARRAY['route_id', 'depot_id'],
        '{"mismatch_threshold_cents": 1}'::jsonb,
        TRUE
    )
ON CONFLICT (metric_key) DO NOTHING;
