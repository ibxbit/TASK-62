-- =============================================================================
-- Migration 001: Auth extensions
-- Adds lockout support to auth.users and activity/reauth tracking to sessions.
-- Apply after db/schema.sql.
-- =============================================================================

-- Brute-force lockout columns
ALTER TABLE auth.users
    ADD COLUMN IF NOT EXISTS failed_login_attempts INTEGER     NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS locked_until          TIMESTAMPTZ;

-- Session activity & re-auth tracking columns
ALTER TABLE auth.sessions
    ADD COLUMN IF NOT EXISTS last_activity_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN IF NOT EXISTS last_reauth_at   TIMESTAMPTZ;

-- Supporting indexes
CREATE INDEX IF NOT EXISTS idx_users_locked_until
    ON auth.users(locked_until)
    WHERE locked_until IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_sessions_last_activity
    ON auth.sessions(last_activity_at)
    WHERE revoked_at IS NULL;
