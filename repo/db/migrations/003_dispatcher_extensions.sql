-- =============================================================================
-- Migration 003: Dispatcher extensions
-- Adds: trip_conflicts, dispatcher_notes
-- Apply after: 002_ops_extensions.sql
-- =============================================================================

-- ---------------------------------------------------------------------------
-- 1. Trip conflicts — persisted records of detected scheduling issues
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ops.trip_conflicts (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    conflict_type    VARCHAR(32) NOT NULL
                                 CHECK (conflict_type IN (
                                     'driver_overlap',
                                     'route_headway',
                                     'unassigned_approaching'
                                 )),
    trip_id_1        UUID        NOT NULL REFERENCES ops.trips(id) ON DELETE CASCADE,
    -- trip_id_2 is NULL for single-trip conflicts (e.g. unassigned_approaching)
    trip_id_2        UUID        REFERENCES ops.trips(id) ON DELETE CASCADE,
    description      TEXT        NOT NULL,
    severity         VARCHAR(16) NOT NULL DEFAULT 'warning'
                                 CHECK (severity IN ('warning', 'critical')),
    status           VARCHAR(16) NOT NULL DEFAULT 'open'
                                 CHECK (status IN ('open', 'acknowledged', 'resolved')),
    detected_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    acknowledged_at  TIMESTAMPTZ,
    acknowledged_by  UUID        REFERENCES auth.users(id),
    resolved_at      TIMESTAMPTZ,
    resolved_by      UUID        REFERENCES auth.users(id),
    notes            TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Fast lookup of open conflicts by trip
CREATE INDEX IF NOT EXISTS idx_conflicts_trip_1_open
    ON ops.trip_conflicts(trip_id_1, detected_at DESC)
    WHERE status = 'open';

CREATE INDEX IF NOT EXISTS idx_conflicts_trip_2_open
    ON ops.trip_conflicts(trip_id_2)
    WHERE status = 'open' AND trip_id_2 IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_conflicts_status
    ON ops.trip_conflicts(status, detected_at DESC);

CREATE INDEX IF NOT EXISTS idx_conflicts_severity
    ON ops.trip_conflicts(severity, status);

-- ---------------------------------------------------------------------------
-- 2. Dispatcher notes — free-form operational notes attached to trips
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ops.dispatcher_notes (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id     UUID        NOT NULL REFERENCES ops.trips(id) ON DELETE CASCADE,
    note        TEXT        NOT NULL,
    note_type   VARCHAR(16) NOT NULL DEFAULT 'general'
                            CHECK (note_type IN ('general', 'conflict', 'override', 'handoff', 'system')),
    created_by  UUID        NOT NULL REFERENCES auth.users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_dispatcher_notes_trip
    ON ops.dispatcher_notes(trip_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- 3. Trigger
-- ---------------------------------------------------------------------------
CREATE TRIGGER trg_set_updated_at_trip_conflicts
    BEFORE UPDATE ON ops.trip_conflicts
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
