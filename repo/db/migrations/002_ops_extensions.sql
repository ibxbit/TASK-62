-- =============================================================================
-- Migration 002: Operations extensions
-- Adds: trip_calendars, entity versioning, scheduled status,
--       depots, rollout plans/stages, depot config assignments
-- Apply after: 001_auth_extensions.sql
-- =============================================================================

-- ---------------------------------------------------------------------------
-- 1. Extend ops.routes status to include 'scheduled'
-- ---------------------------------------------------------------------------
ALTER TABLE ops.routes DROP CONSTRAINT IF EXISTS routes_status_check;
ALTER TABLE ops.routes
    ADD CONSTRAINT routes_status_check
    CHECK (status IN ('draft', 'active', 'inactive', 'scheduled'));

-- Entity version counter (increments on every mutation for optimistic locking)
ALTER TABLE ops.routes
    ADD COLUMN IF NOT EXISTS entity_version  INTEGER     NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS effective_from  TIMESTAMPTZ;

-- ---------------------------------------------------------------------------
-- 3. Trip calendars — defines the days a trip pattern operates
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ops.trip_calendars (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name          VARCHAR(128) NOT NULL,
    description   TEXT,
    -- days_of_week: 0=Sunday … 6=Saturday; empty means all days
    days_of_week  SMALLINT[]  NOT NULL DEFAULT '{0,1,2,3,4,5,6}'::SMALLINT[],
    valid_from    DATE        NOT NULL,
    valid_to      DATE,
    -- exception_dates: {"included": ["2024-12-25"], "excluded": ["2024-01-01"]}
    exception_dates JSONB     NOT NULL DEFAULT '{"included":[],"excluded":[]}'::JSONB,
    deleted_at    TIMESTAMPTZ,
    created_by    UUID        NOT NULL REFERENCES auth.users(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT calendar_valid_range CHECK (valid_to IS NULL OR valid_to > valid_from)
);

CREATE INDEX IF NOT EXISTS idx_calendars_valid_range
    ON ops.trip_calendars(valid_from, valid_to) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- 2. Extend ops.trips status to include 'scheduled'
-- ---------------------------------------------------------------------------
ALTER TABLE ops.trips DROP CONSTRAINT IF EXISTS trips_status_check;
ALTER TABLE ops.trips
    ADD CONSTRAINT trips_status_check
    CHECK (status IN ('scheduled','in_progress','completed','cancelled','draft','published'));

ALTER TABLE ops.trips
    ADD COLUMN IF NOT EXISTS entity_version  INTEGER     NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS effective_from  TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS calendar_id     UUID        REFERENCES ops.trip_calendars(id) ON DELETE SET NULL;

-- ---------------------------------------------------------------------------
-- 4. Depots — physical locations used to scope gradual rollouts
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ops.depots (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    code        VARCHAR(32) NOT NULL UNIQUE,
    name        VARCHAR(128) NOT NULL,
    is_active   BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_depots_active ON ops.depots(is_active);

-- ---------------------------------------------------------------------------
-- 5. Rollout plans — gradual config activation plans
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ops.rollout_plans (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    config_version_id UUID        NOT NULL REFERENCES ops.config_versions(id) ON DELETE RESTRICT,
    status            VARCHAR(16) NOT NULL DEFAULT 'pending'
                                  CHECK (status IN ('pending','active','completed','cancelled')),
    total_depots      INT         NOT NULL DEFAULT 0,
    current_stage     INT         NOT NULL DEFAULT 0,
    created_by        UUID        NOT NULL REFERENCES auth.users(id),
    notes             TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_rollout_plans_version
    ON ops.rollout_plans(config_version_id);

-- ---------------------------------------------------------------------------
-- 6. Rollout stages — each step in the gradual activation
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ops.rollout_stages (
    id                 UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    plan_id            UUID        NOT NULL REFERENCES ops.rollout_plans(id) ON DELETE CASCADE,
    stage_number       SMALLINT    NOT NULL CHECK (stage_number > 0),
    -- target_percentage is documentation; actual scope is defined by depot_ids
    target_percentage  SMALLINT    NOT NULL CHECK (target_percentage BETWEEN 1 AND 100),
    depot_ids          UUID[]      NOT NULL DEFAULT '{}'::UUID[],
    status             VARCHAR(16) NOT NULL DEFAULT 'pending'
                                   CHECK (status IN ('pending','active','completed','cancelled')),
    scheduled_at       TIMESTAMPTZ,         -- auto-activate when NULL means manual
    activated_at       TIMESTAMPTZ,
    activated_by       UUID        REFERENCES auth.users(id),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (plan_id, stage_number)
);

CREATE INDEX IF NOT EXISTS idx_rollout_stages_plan
    ON ops.rollout_stages(plan_id, stage_number);

CREATE INDEX IF NOT EXISTS idx_rollout_stages_scheduled
    ON ops.rollout_stages(scheduled_at)
    WHERE status = 'pending' AND scheduled_at IS NOT NULL;

-- ---------------------------------------------------------------------------
-- 7. Depot config assignments — tracks which config version is live per depot
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ops.depot_config_assignments (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    depot_id          UUID        NOT NULL REFERENCES ops.depots(id) ON DELETE RESTRICT,
    template_id       UUID        NOT NULL REFERENCES ops.config_templates(id) ON DELETE RESTRICT,
    config_version_id UUID        NOT NULL REFERENCES ops.config_versions(id) ON DELETE RESTRICT,
    rollout_stage_id  UUID        REFERENCES ops.rollout_stages(id) ON DELETE SET NULL,
    assigned_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    assigned_by       UUID        NOT NULL REFERENCES auth.users(id),
    UNIQUE (depot_id, template_id)   -- one active config per template per depot
);

CREATE INDEX IF NOT EXISTS idx_depot_assignments_depot
    ON ops.depot_config_assignments(depot_id);
CREATE INDEX IF NOT EXISTS idx_depot_assignments_version
    ON ops.depot_config_assignments(config_version_id);

-- ---------------------------------------------------------------------------
-- 8. Attach updated_at triggers to new tables
-- ---------------------------------------------------------------------------
CREATE TRIGGER trg_set_updated_at_trip_calendars
    BEFORE UPDATE ON ops.trip_calendars
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_set_updated_at_depots
    BEFORE UPDATE ON ops.depots
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_set_updated_at_rollout_plans
    BEFORE UPDATE ON ops.rollout_plans
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_set_updated_at_rollout_stages
    BEFORE UPDATE ON ops.rollout_stages
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
