-- ============================================================
-- Migration 012 — Pluggable notification channel infrastructure
-- ============================================================
--
-- Adds two tables that support external channel delivery
-- (email, SMS, WeCom) alongside the existing in-app inbox:
--
--   channel_preferences  — per-user opt-in per channel with
--                          the delivery address (email addr,
--                          phone number, WeCom user ID).
--
--   channel_deliveries   — records each external dispatch
--                          attempt with outcome for audit /
--                          retry visibility.
--
-- Channels are DISABLED by default.  A user must explicitly
-- call PUT /notifications/channels/{channel} to opt in.
-- An adapter only activates when its connector URL env var is
-- set; without it, is_available() returns false and no
-- network calls are made.
-- ============================================================

-- ---- Per-user channel preferences ----
CREATE TABLE IF NOT EXISTS notifications.channel_preferences (
    user_id          UUID        NOT NULL
                     REFERENCES auth.users(id) ON DELETE CASCADE,
    channel          VARCHAR(32) NOT NULL
                     CHECK (channel IN ('email', 'sms', 'wecom')),
    enabled          BOOLEAN     NOT NULL DEFAULT TRUE,
    -- The address at which this user wants to receive messages
    -- on this channel: email address | E.164 phone | WeCom user_id
    channel_address  TEXT        NOT NULL
                     CHECK (char_length(trim(channel_address)) > 0),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, channel)
);

-- ---- External dispatch tracking ----
-- UNIQUE (event_id, user_id, channel) prevents duplicate dispatch
-- on bus retries; ON CONFLICT DO NOTHING is used in the bus.
CREATE TABLE IF NOT EXISTS notifications.channel_deliveries (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id     UUID        NOT NULL
                 REFERENCES notifications.events(id) ON DELETE CASCADE,
    user_id      UUID        NOT NULL
                 REFERENCES auth.users(id)  ON DELETE CASCADE,
    channel      VARCHAR(32) NOT NULL,
    status       TEXT        NOT NULL DEFAULT 'sent'
                 CHECK (status IN ('sent', 'failed', 'skipped')),
    error_msg    TEXT,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (event_id, user_id, channel)
);

CREATE INDEX IF NOT EXISTS idx_channel_deliveries_event
    ON notifications.channel_deliveries (event_id);

CREATE INDEX IF NOT EXISTS idx_channel_deliveries_failed
    ON notifications.channel_deliveries (attempted_at DESC)
    WHERE status = 'failed';
