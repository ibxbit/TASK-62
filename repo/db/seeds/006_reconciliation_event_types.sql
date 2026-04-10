-- ============================================================
-- Seed 006 — Reconciliation notification event types
-- ============================================================
-- Inserts event_definitions for the three reconciliation events.
-- Severities:
--   completed (no discrepancies)   → info
--   completed (with discrepancies) → warning   (set per-event in payload)
--   discrepancy_found              → warning
--   high_discrepancy               → critical
-- Idempotent: ON CONFLICT DO NOTHING
-- ============================================================

INSERT INTO notifications.event_definitions
    (event_type, domain, description, severity)
VALUES
    ('payments.reconciliation.completed',
     'payments',
     'A daily reconciliation run completed. Payload includes run_id, '
     'run_date, matched_count, discrepancy_count, and total_expected vs total_collected.',
     'info'),

    ('payments.reconciliation.discrepancy_found',
     'payments',
     'One or more discrepancies were detected during reconciliation: '
     'amount mismatches, missing transactions, or unexpected statement entries.',
     'warning'),

    ('payments.reconciliation.high_discrepancy',
     'payments',
     'Reconciliation found a high volume of discrepancies (> 10 items or > 5% of records). '
     'Immediate review required.',
     'critical')

ON CONFLICT (event_type) DO NOTHING;
