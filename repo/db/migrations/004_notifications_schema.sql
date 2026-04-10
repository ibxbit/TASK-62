-- =============================================================================
-- Migration 004: Notification delivery pipeline
-- =============================================================================
-- Extends the notifications schema with:
--   • severity column on event_definitions and events
--   • processed_at flag on events (drives the fan-out bus)
--   • notifications.preferences  — per-user DND settings
--   • notifications.subscriptions — user opt-in per event type
--   • notifications.deliveries   — in-app delivery + read tracking
-- =============================================================================

BEGIN;

-- ── Extend existing tables ────────────────────────────────────────────────────

-- Default severity per event type (can be overridden per-event via payload)
ALTER TABLE notifications.event_definitions
    ADD COLUMN IF NOT EXISTS severity TEXT NOT NULL DEFAULT 'info'
        CHECK (severity IN ('info', 'warning', 'critical'));

-- Per-event severity (resolved by bus from payload or event_definitions)
-- processed_at NULL  ⟹ pending fan-out
ALTER TABLE notifications.events
    ADD COLUMN IF NOT EXISTS severity     TEXT NOT NULL DEFAULT 'info'
        CHECK (severity IN ('info', 'warning', 'critical')),
    ADD COLUMN IF NOT EXISTS processed_at TIMESTAMPTZ;

-- Partial index so the bus can cheaply find pending work
CREATE INDEX IF NOT EXISTS idx_events_pending
    ON notifications.events (created_at ASC)
    WHERE processed_at IS NULL;

-- ── Per-user DND preferences ──────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS notifications.preferences (
    user_id      UUID    PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
    dnd_enabled  BOOLEAN NOT NULL DEFAULT FALSE,
    -- UTC time window for DND. NULL + dnd_enabled = TRUE means all-day DND.
    dnd_start    TIME,
    dnd_end      TIME,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE notifications.preferences IS
    'Per-user DND settings. Critical-severity events bypass DND regardless.';

-- ── Event subscriptions ───────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS notifications.subscriptions (
    id           UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID    NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    event_type   TEXT    NOT NULL
                         REFERENCES notifications.event_definitions(event_type)
                         ON DELETE CASCADE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, event_type)
);

-- Looked up once per fan-out cycle keyed on event_type
CREATE INDEX IF NOT EXISTS idx_subscriptions_event_type
    ON notifications.subscriptions (event_type);

COMMENT ON TABLE notifications.subscriptions IS
    'User opt-in per event type. sys.announcement is exempt — it fans out to all users.';

-- ── In-app delivery tracking ──────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS notifications.deliveries (
    id           UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id     UUID    NOT NULL REFERENCES notifications.events(id)   ON DELETE CASCADE,
    user_id      UUID    NOT NULL REFERENCES auth.users(id)             ON DELETE CASCADE,
    -- Lifecycle: delivered → read → dismissed
    status       TEXT    NOT NULL DEFAULT 'delivered'
                         CHECK (status IN ('delivered', 'read', 'dismissed')),
    delivered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    read_at      TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (event_id, user_id)   -- idempotent inserts from the bus
);

-- Efficient unread-badge and inbox listing per user
CREATE INDEX IF NOT EXISTS idx_deliveries_user_unread
    ON notifications.deliveries (user_id, created_at DESC)
    WHERE status = 'delivered';

COMMENT ON TABLE notifications.deliveries IS
    'One row per (event, user). Created by the fan-out bus; updated by user actions.';

COMMIT;
