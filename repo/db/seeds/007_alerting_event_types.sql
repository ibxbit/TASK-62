-- ============================================================
-- Seed 007 — Alerting notification event types
-- ============================================================
-- Four event_definitions for the anomaly alerting system.
--
-- Anomaly events (system-generated, routed to subscribers):
--   alerts.anomaly.reconciliation_mismatch  warning/critical
--   alerts.anomaly.kpi_deviation            warning/critical
--
-- Workflow events (actor-generated, inform interested parties):
--   alerts.alert.acknowledged               info
--   alerts.alert.closed                     info
--
-- Severity in the definition is the *default*; the actual delivered
-- severity comes from the event payload's "severity" field
-- (the bus's COALESCE(payload->>'severity', definition.severity) logic).
--
-- Idempotent: ON CONFLICT DO NOTHING
-- ============================================================

INSERT INTO notifications.event_definitions
    (event_type, domain, description, severity)
VALUES
    ('alerts.anomaly.reconciliation_mismatch',
     'alerting',
     'A reconciliation run detected anomalous discrepancies. '
     'Payload: alert_id, run_id, run_date, discrepancy_count, '
     'amount_mismatches, missing_from_statement, extra_in_statement, duplicates.',
     'warning'),

    ('alerts.anomaly.kpi_deviation',
     'alerting',
     'A KPI metric value has deviated significantly from its recent baseline. '
     'Payload: alert_id, metric_id, metric_key, current_value, '
     'baseline_avg, deviation_pct, threshold_pct.',
     'warning'),

    ('alerts.alert.acknowledged',
     'alerting',
     'An anomaly alert has been acknowledged by an operator. '
     'Payload: alert_id, acknowledged_by.',
     'info'),

    ('alerts.alert.closed',
     'alerting',
     'An anomaly alert has been closed. '
     'Payload: alert_id, closed_by, reason.',
     'info')

ON CONFLICT (event_type) DO NOTHING;
