-- =============================================================================
-- Seed 002: RBAC — Roles, Permissions, and Role-Permission mappings
-- =============================================================================
-- Idempotent: all inserts use ON CONFLICT DO NOTHING.
-- Permission strings must exactly match Permission::as_str() in src/rbac/permissions.rs.
-- Apply after: db/schema.sql + db/migrations/001_auth_extensions.sql
-- =============================================================================

-- ---------------------------------------------------------------------------
-- 1. Roles
-- ---------------------------------------------------------------------------
INSERT INTO auth.roles (id, name, description) VALUES
    ('a0000001-0000-4000-8000-000000000001', 'operations_admin',
        'Full control over operations, user management, and audit access'),
    ('a0000001-0000-4000-8000-000000000002', 'dispatcher',
        'Operational read/write access for routes, stops, and trips'),
    ('a0000001-0000-4000-8000-000000000003', 'finance_analyst',
        'Full payments domain access including reconciliation and reporting'),
    ('a0000001-0000-4000-8000-000000000004', 'staff_user',
        'Read-only operational access and own inbox')
ON CONFLICT (name) DO NOTHING;


-- ---------------------------------------------------------------------------
-- 2. Permissions  (domain | action | name)
-- ---------------------------------------------------------------------------
INSERT INTO auth.permissions (id, name, domain, action) VALUES
    -- Ops: Routes
    ('b0000001-0000-4000-8000-000000000001', 'ops:routes:read',   'ops', 'read'),
    ('b0000001-0000-4000-8000-000000000002', 'ops:routes:write',  'ops', 'write'),
    ('b0000001-0000-4000-8000-000000000003', 'ops:routes:delete', 'ops', 'delete'),
    -- Ops: Stops
    ('b0000001-0000-4000-8000-000000000004', 'ops:stops:read',    'ops', 'read'),
    ('b0000001-0000-4000-8000-000000000005', 'ops:stops:write',   'ops', 'write'),
    ('b0000001-0000-4000-8000-000000000006', 'ops:stops:delete',  'ops', 'delete'),
    -- Ops: Trips
    ('b0000001-0000-4000-8000-000000000007', 'ops:trips:read',    'ops', 'read'),
    ('b0000001-0000-4000-8000-000000000008', 'ops:trips:write',   'ops', 'write'),
    ('b0000001-0000-4000-8000-000000000009', 'ops:trips:delete',  'ops', 'delete'),
    -- Ops: Config
    ('b0000001-0000-4000-8000-000000000010', 'ops:config:read',    'ops', 'read'),
    ('b0000001-0000-4000-8000-000000000011', 'ops:config:write',   'ops', 'write'),
    ('b0000001-0000-4000-8000-000000000012', 'ops:config:publish', 'ops', 'admin'),
    -- Notifications
    ('b0000001-0000-4000-8000-000000000013', 'notifications:inbox:read',               'notifications', 'read'),
    ('b0000001-0000-4000-8000-000000000014', 'notifications:all:read',                 'notifications', 'admin'),
    ('b0000001-0000-4000-8000-000000000015', 'notifications:subscriptions:manage',     'notifications', 'write'),
    ('b0000001-0000-4000-8000-000000000016', 'notifications:dnd:manage',               'notifications', 'write'),
    -- Payments: Transactions
    ('b0000001-0000-4000-8000-000000000017', 'payments:transactions:read',  'payments', 'read'),
    ('b0000001-0000-4000-8000-000000000018', 'payments:transactions:write', 'payments', 'write'),
    -- Payments: Refunds
    ('b0000001-0000-4000-8000-000000000019', 'payments:refunds:read',    'payments', 'read'),
    ('b0000001-0000-4000-8000-000000000020', 'payments:refunds:write',   'payments', 'write'),
    ('b0000001-0000-4000-8000-000000000021', 'payments:refunds:approve', 'payments', 'admin'),
    -- Payments: Reconciliation
    ('b0000001-0000-4000-8000-000000000022', 'payments:reconciliation:read', 'payments', 'read'),
    ('b0000001-0000-4000-8000-000000000023', 'payments:reconciliation:run',  'payments', 'admin'),
    -- Payments: Statements
    ('b0000001-0000-4000-8000-000000000024', 'payments:statements:read',   'payments', 'read'),
    ('b0000001-0000-4000-8000-000000000025', 'payments:statements:import', 'payments', 'write'),
    -- Reporting
    ('b0000001-0000-4000-8000-000000000026', 'reporting:reports:read',    'reporting', 'read'),
    ('b0000001-0000-4000-8000-000000000027', 'reporting:reports:export',  'reporting', 'write'),
    ('b0000001-0000-4000-8000-000000000028', 'reporting:metrics:manage',  'reporting', 'admin'),
    -- Audit
    ('b0000001-0000-4000-8000-000000000029', 'audit:log:read', 'audit', 'read'),
    -- User management
    ('b0000001-0000-4000-8000-000000000030', 'users:users:read',   'auth', 'read'),
    ('b0000001-0000-4000-8000-000000000031', 'users:users:write',  'auth', 'write'),
    ('b0000001-0000-4000-8000-000000000032', 'users:users:delete', 'auth', 'delete'),
    ('b0000001-0000-4000-8000-000000000033', 'users:roles:read',   'auth', 'read'),
    ('b0000001-0000-4000-8000-000000000034', 'users:roles:write',  'auth', 'admin')
ON CONFLICT (name) DO NOTHING;


-- ---------------------------------------------------------------------------
-- 3. Role-Permission mappings
-- Each block mirrors the exact policy in src/rbac/permissions.rs :: build_map().
-- ---------------------------------------------------------------------------

-- ---- Operations Admin (27 permissions) ----
INSERT INTO auth.role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM   auth.roles r
JOIN   auth.permissions p ON p.name IN (
    -- Full ops
    'ops:routes:read',   'ops:routes:write',  'ops:routes:delete',
    'ops:stops:read',    'ops:stops:write',   'ops:stops:delete',
    'ops:trips:read',    'ops:trips:write',   'ops:trips:delete',
    'ops:config:read',   'ops:config:write',  'ops:config:publish',
    -- Full notifications
    'notifications:inbox:read', 'notifications:all:read',
    'notifications:subscriptions:manage', 'notifications:dnd:manage',
    -- Payment visibility + refund approval
    'payments:transactions:read',
    'payments:refunds:read',    'payments:refunds:approve',
    'payments:reconciliation:read',
    -- Reporting
    'reporting:reports:read', 'reporting:reports:export',
    -- Audit
    'audit:log:read',
    -- User management (no roles:write)
    'users:users:read', 'users:users:write', 'users:users:delete', 'users:roles:read'
)
WHERE r.name = 'operations_admin'
ON CONFLICT DO NOTHING;

-- ---- Dispatcher (11 permissions) ----
INSERT INTO auth.role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM   auth.roles r
JOIN   auth.permissions p ON p.name IN (
    -- Ops read/write (no delete, no config publish)
    'ops:routes:read', 'ops:routes:write',
    'ops:stops:read',  'ops:stops:write',
    'ops:trips:read',  'ops:trips:write',
    'ops:config:read',
    -- Own notifications
    'notifications:inbox:read',
    'notifications:subscriptions:manage',
    'notifications:dnd:manage',
    -- Operational reporting
    'reporting:reports:read'
)
WHERE r.name = 'dispatcher'
ON CONFLICT DO NOTHING;

-- ---- Finance Analyst (16 permissions) ----
INSERT INTO auth.role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM   auth.roles r
JOIN   auth.permissions p ON p.name IN (
    -- Ops read (reference data)
    'ops:routes:read', 'ops:trips:read',
    -- Own notifications
    'notifications:inbox:read',
    'notifications:subscriptions:manage',
    'notifications:dnd:manage',
    -- Full payments domain
    'payments:transactions:read',  'payments:transactions:write',
    'payments:refunds:read',       'payments:refunds:write',     'payments:refunds:approve',
    'payments:reconciliation:read','payments:reconciliation:run',
    'payments:statements:read',    'payments:statements:import',
    -- Reporting with export
    'reporting:reports:read', 'reporting:reports:export'
)
WHERE r.name = 'finance_analyst'
ON CONFLICT DO NOTHING;

-- ---- Staff User (5 permissions) ----
INSERT INTO auth.role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM   auth.roles r
JOIN   auth.permissions p ON p.name IN (
    -- Read-only ops
    'ops:routes:read', 'ops:trips:read',
    -- Own inbox + DND
    'notifications:inbox:read', 'notifications:dnd:manage',
    -- Basic reporting
    'reporting:reports:read'
)
WHERE r.name = 'staff_user'
ON CONFLICT DO NOTHING;
