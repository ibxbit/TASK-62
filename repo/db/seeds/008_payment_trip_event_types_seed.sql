-- =============================================================================
-- Seed 008: Payment and simplified trip event type definitions
-- =============================================================================
-- Adds the payment lifecycle event types and a short-form trip.completed
-- alias used by subscription tests and the notification bus.
-- Idempotent: ON CONFLICT DO NOTHING.
-- =============================================================================

INSERT INTO notifications.event_definitions
    (event_type, domain, description, severity)
VALUES
    ('payment.captured',
     'payments',
     'A payment was successfully captured by the gateway',
     'info'),

    ('payment.failed',
     'payments',
     'A payment attempt failed at the gateway',
     'warning'),

    ('payment.refunded',
     'payments',
     'A refund was successfully processed for a transaction',
     'info'),

    ('trip.completed',
     'ops',
     'A transit trip was marked as completed (short-form alias for ops.trip.completed)',
     'info')

ON CONFLICT (event_type) DO NOTHING;
