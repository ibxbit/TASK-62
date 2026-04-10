-- =============================================================================
-- Seed 004: Notification event types — severity + new types
-- =============================================================================
-- Updates severity on the 7 dispatcher event types seeded in 003.
-- Inserts 5 new event types for requests and system announcements.
-- Idempotent: UPDATE sets severity; INSERT ON CONFLICT skips duplicates.
-- =============================================================================

-- ── Update severity on existing dispatcher event types ────────────────────────

UPDATE notifications.event_definitions
SET    severity = 'info'
WHERE  event_type IN (
    'ops.trip.modified',
    'ops.trip.driver_assigned',
    'ops.trip.started',
    'ops.trip.completed',
    'ops.trip.cancelled'
);

-- Conflicts and approaching trips are warnings by default.
-- Critical-severity conflicts include their severity in the event payload,
-- allowing the bus to override for DND bypass on a per-event basis.
UPDATE notifications.event_definitions
SET    severity = 'warning'
WHERE  event_type IN (
    'ops.trip.conflict_detected',
    'ops.trip.start_approaching'
);

-- ── New event types ───────────────────────────────────────────────────────────

INSERT INTO notifications.event_definitions
    (event_type, domain, description, severity)
VALUES
    ('ops.request.submitted',
     'ops',
     'A leave/schedule change request was submitted by a staff member',
     'info'),

    ('ops.request.approved',
     'ops',
     'A pending request was approved by a dispatcher or admin',
     'info'),

    ('ops.request.rejected',
     'ops',
     'A pending request was rejected',
     'warning'),

    ('ops.request.changed',
     'ops',
     'An existing request was modified after initial submission',
     'info'),

    ('sys.announcement',
     'sys',
     'A system-wide or role-targeted announcement from operations admin. '
     'Fan-out bypasses subscriptions — all targeted users receive it. '
     'Payload severity field overrides the event_definitions default.',
     'info')

ON CONFLICT (event_type) DO NOTHING;
