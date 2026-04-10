-- =============================================================================
-- Migration 005: Subscription rules, DND queueing, default DND window
-- =============================================================================
-- Changes:
--   1. notifications.deliveries
--        - delivered_at becomes nullable (queued rows have no delivery time yet)
--        - status gains 'queued' (notification held during DND for later delivery)
--   2. notifications.preferences
--        - default DND window: 22:00–07:00 UTC, enabled by default for new rows
--        - existing unconfigured rows backfilled to the same default
--   3. notifications.subscription_rules  (NEW)
--        - four rule types: keyword | topic | entity_threshold | spike
--        - cooldown_minutes prevents rapid re-firing
--   4. notifications.event_definitions
--        - add sys.rule_alert for rule-triggered synthetic alerts
-- =============================================================================

BEGIN;

-- ── 1. deliveries: nullable delivered_at + 'queued' status ───────────────────

ALTER TABLE notifications.deliveries
    ALTER COLUMN delivered_at DROP NOT NULL,
    ALTER COLUMN delivered_at SET DEFAULT NULL;

ALTER TABLE notifications.deliveries
    DROP CONSTRAINT IF EXISTS deliveries_status_check,
    ADD  CONSTRAINT deliveries_status_check
         CHECK (status IN ('queued', 'delivered', 'read', 'dismissed'));

-- Fast lookup for flushing queued deliveries on DND window end
CREATE INDEX IF NOT EXISTS idx_deliveries_queued
    ON notifications.deliveries (user_id)
    WHERE status = 'queued';

-- ── 2. preferences: default DND window 22:00–07:00 UTC ───────────────────────

ALTER TABLE notifications.preferences
    ALTER COLUMN dnd_enabled SET DEFAULT TRUE,
    ALTER COLUMN dnd_start   SET DEFAULT '22:00:00',
    ALTER COLUMN dnd_end     SET DEFAULT '07:00:00';

-- Back-fill only rows that were created with dnd_enabled = FALSE and no window
-- (i.e. the row was auto-created by get_preferences but the user never customised it)
UPDATE notifications.preferences
SET    dnd_enabled = TRUE,
       dnd_start   = '22:00:00',
       dnd_end     = '07:00:00',
       updated_at  = now()
WHERE  dnd_enabled = FALSE
  AND  dnd_start   IS NULL
  AND  dnd_end     IS NULL;

-- ── 3. subscription_rules table ───────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS notifications.subscription_rules (
    id                UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id           UUID    NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,

    -- Human-readable label for this rule
    rule_name         TEXT    NOT NULL CHECK (char_length(trim(rule_name)) > 0),

    -- Determines which evaluation path the rule takes
    rule_type         TEXT    NOT NULL
                              CHECK (rule_type IN ('keyword', 'topic',
                                                   'entity_threshold', 'spike')),
    is_enabled        BOOLEAN NOT NULL DEFAULT TRUE,

    -- Type-specific configuration (see rule_type docs below)
    --
    -- keyword:
    --   { "keywords": ["str", ...], "match_mode": "any"|"all",
    --     "match_fields": ["payload"]  }   ← match_mode/fields are optional
    --
    -- topic:
    --   { "pattern": "ops.trip.*" }          ← OR
    --   { "topics": ["ops.trip", "ops.request"] }
    --
    -- entity_threshold:
    --   { "metric": "open_conflicts"|"unassigned_trips"|"active_trips",
    --     "threshold": <int>, "operator": ">"|">="|"=="|"<="|"<",
    --     "entity_id": "<uuid>" }   ← entity_id optional; applies per-trip for open_conflicts
    --
    -- spike:
    --   { "metric": "conflict_rate"|"cancellation_rate"|"driver_assignment_rate",
    --     "threshold_pct": <float>, "window_minutes": <int (default 10)>,
    --     "direction": "up"|"down"|"either" }
    config            JSONB   NOT NULL DEFAULT '{}',

    -- Override the alert severity emitted when this rule fires.
    -- NULL = use the matched event's severity (keyword/topic) or 'warning' (threshold/spike).
    severity_override TEXT    CHECK (severity_override IN ('info', 'warning', 'critical')),

    -- Minimum gap between successive firings of this rule.
    -- Acts as a cooldown to prevent alert storms.
    cooldown_minutes  INT     NOT NULL DEFAULT 15 CHECK (cooldown_minutes >= 1),

    -- Set to now() each time the rule fires; compared against cooldown_minutes.
    last_triggered_at TIMESTAMPTZ,

    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_subscription_rules_user
    ON notifications.subscription_rules (user_id);

-- Bus loads rules by type on every poll; partial index keeps it cheap.
CREATE INDEX IF NOT EXISTS idx_subscription_rules_active_type
    ON notifications.subscription_rules (rule_type)
    WHERE is_enabled = TRUE;

COMMENT ON TABLE notifications.subscription_rules IS
    'User-defined alert rules evaluated by the event bus. Four types: '
    'keyword and topic rules fire on matching events; '
    'entity_threshold and spike rules fire on periodic DB state checks.';

-- ── 4. sys.rule_alert event type ─────────────────────────────────────────────

INSERT INTO notifications.event_definitions (event_type, domain, description, severity)
VALUES (
    'sys.rule_alert',
    'sys',
    'Alert generated when a user-defined subscription rule condition is met. '
    'Payload: { rule_id, rule_name, rule_type, description, severity }. '
    'Delivery is created directly — processed_at is set on insert.',
    'warning'
)
ON CONFLICT (event_type) DO NOTHING;

COMMIT;
