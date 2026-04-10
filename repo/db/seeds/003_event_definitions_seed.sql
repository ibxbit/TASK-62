-- =============================================================================
-- Seed 003: Dispatcher event definitions
-- =============================================================================
-- These entries must be present in notifications.event_definitions before any
-- dispatcher event can be emitted (FK constraint on notifications.events).
-- Idempotent: ON CONFLICT DO NOTHING.
-- =============================================================================

INSERT INTO notifications.event_definitions
    (event_type, domain, description)
VALUES
    ('ops.trip.modified',
     'ops',
     'A dispatcher modified a trip schedule or assignment'),

    ('ops.trip.driver_assigned',
     'ops',
     'A driver was assigned (or reassigned) to a trip'),

    ('ops.trip.started',
     'ops',
     'A trip was marked as in-progress (actual departure recorded)'),

    ('ops.trip.completed',
     'ops',
     'A trip was marked as completed (actual arrival recorded)'),

    ('ops.trip.cancelled',
     'ops',
     'A trip was cancelled by a dispatcher'),

    ('ops.trip.conflict_detected',
     'ops',
     'A scheduling conflict was detected for a trip (driver overlap or headway violation)'),

    ('ops.trip.start_approaching',
     'ops',
     'A trip is approaching its scheduled start time (within 30 minutes)')

ON CONFLICT (event_type) DO NOTHING;
