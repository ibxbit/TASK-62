/// Fine-grained permission model for TransitOps.
///
/// Each `Permission` variant maps 1-to-1 to a string of the form
/// `{domain}:{resource}:{action}`.  These strings are the canonical names
/// stored in `auth.permissions` and used in error messages.
///
/// Role-to-permission mapping is a static, code-defined truth (see `build_map`).
/// The DB `auth.role_permissions` table mirrors this mapping as reference data
/// for the audit trail; it is NOT the enforcement source.
use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::OnceLock,
};

// ============================================================
// Role name constants — must match auth.roles.name in DB
// ============================================================
pub const ROLE_OPERATIONS_ADMIN: &str = "operations_admin";
pub const ROLE_DISPATCHER:        &str = "dispatcher";
pub const ROLE_FINANCE_ANALYST:   &str = "finance_analyst";
pub const ROLE_STAFF_USER:        &str = "staff_user";

// ============================================================
// Permission enum (34 variants)
// ============================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    // ---- Ops: Routes ----
    OpsRoutesRead,
    OpsRoutesWrite,
    OpsRoutesDelete,
    // ---- Ops: Stops ----
    OpsStopsRead,
    OpsStopsWrite,
    OpsStopsDelete,
    // ---- Ops: Trips ----
    OpsTripsRead,
    OpsTripsWrite,
    OpsTripsDelete,
    // ---- Ops: Configuration ----
    OpsConfigRead,
    OpsConfigWrite,
    OpsConfigPublish,
    // ---- Notifications ----
    NotificationsInboxRead,           // own inbox only
    NotificationsReadAll,             // admin: see everyone's
    NotificationsSubscriptionsManage,
    NotificationsDndManage,
    SysAnnouncementWrite,             // broadcast system announcements
    // ---- Payments: Transactions ----
    PaymentsTransactionsRead,
    PaymentsTransactionsWrite,
    // ---- Payments: Refunds ----
    PaymentsRefundsRead,
    PaymentsRefundsWrite,
    PaymentsRefundsApprove,
    // ---- Payments: Reconciliation ----
    PaymentsReconciliationRead,
    PaymentsReconciliationRun,
    // ---- Payments: Statements ----
    PaymentsStatementsRead,
    PaymentsStatementsImport,
    // ---- Reporting ----
    ReportingRead,
    ReportingExport,
    ReportingMetricsManage,
    // ---- Alerts ----
    AlertsRead,
    AlertsManage,
    // ---- Audit ----
    AuditRead,
    // ---- User management ----
    UsersRead,
    UsersWrite,
    UsersDelete,
    RolesRead,
    RolesWrite,
}

impl Permission {
    /// Canonical string representation; mirrors `auth.permissions.name` in DB.
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::OpsRoutesRead                   => "ops:routes:read",
            Permission::OpsRoutesWrite                  => "ops:routes:write",
            Permission::OpsRoutesDelete                 => "ops:routes:delete",
            Permission::OpsStopsRead                    => "ops:stops:read",
            Permission::OpsStopsWrite                   => "ops:stops:write",
            Permission::OpsStopsDelete                  => "ops:stops:delete",
            Permission::OpsTripsRead                    => "ops:trips:read",
            Permission::OpsTripsWrite                   => "ops:trips:write",
            Permission::OpsTripsDelete                  => "ops:trips:delete",
            Permission::OpsConfigRead                   => "ops:config:read",
            Permission::OpsConfigWrite                  => "ops:config:write",
            Permission::OpsConfigPublish                => "ops:config:publish",
            Permission::NotificationsInboxRead          => "notifications:inbox:read",
            Permission::NotificationsReadAll            => "notifications:all:read",
            Permission::NotificationsSubscriptionsManage=> "notifications:subscriptions:manage",
            Permission::NotificationsDndManage          => "notifications:dnd:manage",
            Permission::SysAnnouncementWrite            => "sys:announcements:write",
            Permission::PaymentsTransactionsRead        => "payments:transactions:read",
            Permission::PaymentsTransactionsWrite       => "payments:transactions:write",
            Permission::PaymentsRefundsRead             => "payments:refunds:read",
            Permission::PaymentsRefundsWrite            => "payments:refunds:write",
            Permission::PaymentsRefundsApprove          => "payments:refunds:approve",
            Permission::PaymentsReconciliationRead      => "payments:reconciliation:read",
            Permission::PaymentsReconciliationRun       => "payments:reconciliation:run",
            Permission::PaymentsStatementsRead          => "payments:statements:read",
            Permission::PaymentsStatementsImport        => "payments:statements:import",
            Permission::ReportingRead                   => "reporting:reports:read",
            Permission::ReportingExport                 => "reporting:reports:export",
            Permission::ReportingMetricsManage          => "reporting:metrics:manage",
            Permission::AlertsRead                      => "alerts:alerts:read",
            Permission::AlertsManage                    => "alerts:alerts:manage",
            Permission::AuditRead                       => "audit:log:read",
            Permission::UsersRead                       => "users:users:read",
            Permission::UsersWrite                      => "users:users:write",
            Permission::UsersDelete                     => "users:users:delete",
            Permission::RolesRead                       => "users:roles:read",
            Permission::RolesWrite                      => "users:roles:write",
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================
// Role → Permission mapping  (static, code-defined)
// ============================================================

type PermSet = HashSet<Permission>;

static PERMISSIONS_MAP: OnceLock<HashMap<&'static str, PermSet>> = OnceLock::new();

/// Returns the static role-permission map, initialised once on first call.
pub fn role_permissions() -> &'static HashMap<&'static str, PermSet> {
    PERMISSIONS_MAP.get_or_init(build_map)
}

/// Returns `true` if `role` holds `permission` per the static policy.
/// Unknown role names always return `false` (fail-closed).
pub fn has_permission(role: &str, permission: Permission) -> bool {
    role_permissions()
        .get(role)
        .map_or(false, |set| set.contains(&permission))
}

// ============================================================
// Policy definition
// ============================================================
//
// Principle of least privilege:
//  - Every permission is explicitly listed (no wildcard / "all" grants).
//  - Permissions are additive; roles cannot negate another role's grants.
//  - RolesWrite is withheld from all predefined roles — it must be granted
//    only via a separate super-admin provisioning path (out of scope here).
//
// Role summary:
//   Operations Admin  — full ops domain, limited payment read, audit & user management
//   Dispatcher        — ops read/write (no delete/publish), own notifications
//   Finance Analyst   — full payments domain, ops read for reference
//   Staff User        — read-only ops, own inbox, basic reporting

fn build_map() -> HashMap<&'static str, PermSet> {
    use Permission::*;

    let mut m: HashMap<&'static str, PermSet> = HashMap::new();

    // ------------------------------------------------------------------
    // Operations Admin  (27 permissions)
    // ------------------------------------------------------------------
    m.insert(ROLE_OPERATIONS_ADMIN, HashSet::from([
        // Full ops control
        OpsRoutesRead, OpsRoutesWrite, OpsRoutesDelete,
        OpsStopsRead,  OpsStopsWrite,  OpsStopsDelete,
        OpsTripsRead,  OpsTripsWrite,  OpsTripsDelete,
        OpsConfigRead, OpsConfigWrite, OpsConfigPublish,
        // Full notification management + announcement broadcast
        NotificationsInboxRead, NotificationsReadAll,
        NotificationsSubscriptionsManage, NotificationsDndManage,
        SysAnnouncementWrite,
        // Payment visibility + refund approval (no financial write authority)
        PaymentsTransactionsRead,
        PaymentsRefundsRead, PaymentsRefundsApprove,
        PaymentsReconciliationRead,
        // Reporting (including metric definition management)
        ReportingRead, ReportingExport, ReportingMetricsManage,
        // Anomaly alerts — full read + manage (acknowledge / close)
        AlertsRead, AlertsManage,
        // Audit trail visibility
        AuditRead,
        // User management (not RolesWrite — role schema changes require deployment)
        UsersRead, UsersWrite, UsersDelete, RolesRead,
    ]));

    // ------------------------------------------------------------------
    // Dispatcher  (11 permissions)
    // ------------------------------------------------------------------
    m.insert(ROLE_DISPATCHER, HashSet::from([
        // Ops read/write — no delete, no config publish
        OpsRoutesRead, OpsRoutesWrite,
        OpsStopsRead,  OpsStopsWrite,
        OpsTripsRead,  OpsTripsWrite,
        OpsConfigRead,
        // Own notifications
        NotificationsInboxRead,
        NotificationsSubscriptionsManage,
        NotificationsDndManage,
        // Operational reports (read-only)
        ReportingRead,
        // Alert visibility (read-only — ops dispatchers can see alerts but not close them)
        AlertsRead,
    ]));

    // ------------------------------------------------------------------
    // Finance Analyst  (16 permissions)
    // ------------------------------------------------------------------
    m.insert(ROLE_FINANCE_ANALYST, HashSet::from([
        // Ops read — reference data needed for trip-to-payment correlation
        OpsRoutesRead, OpsTripsRead,
        // Own notifications
        NotificationsInboxRead,
        NotificationsSubscriptionsManage,
        NotificationsDndManage,
        // Full payments domain
        PaymentsTransactionsRead, PaymentsTransactionsWrite,
        PaymentsRefundsRead,      PaymentsRefundsWrite, PaymentsRefundsApprove,
        PaymentsReconciliationRead, PaymentsReconciliationRun,
        PaymentsStatementsRead,   PaymentsStatementsImport,
        // Reporting with export (for external reconciliation review)
        ReportingRead, ReportingExport, ReportingMetricsManage,
        // Anomaly alerts — full read + manage
        AlertsRead, AlertsManage,
    ]));

    // ------------------------------------------------------------------
    // Staff User  (5 permissions)
    // ------------------------------------------------------------------
    m.insert(ROLE_STAFF_USER, HashSet::from([
        // Read-only operational access
        OpsRoutesRead, OpsTripsRead,
        // Own inbox, DND, and subscription management
        NotificationsInboxRead, NotificationsDndManage, NotificationsSubscriptionsManage,
        // Basic reporting
        ReportingRead,
    ]));

    m
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_admin_has_full_ops() {
        for perm in [
            Permission::OpsRoutesRead,  Permission::OpsRoutesWrite,  Permission::OpsRoutesDelete,
            Permission::OpsStopsRead,   Permission::OpsStopsWrite,   Permission::OpsStopsDelete,
            Permission::OpsTripsRead,   Permission::OpsTripsWrite,   Permission::OpsTripsDelete,
            Permission::OpsConfigRead,  Permission::OpsConfigWrite,  Permission::OpsConfigPublish,
        ] {
            assert!(has_permission(ROLE_OPERATIONS_ADMIN, perm), "ops_admin missing {:?}", perm);
        }
    }

    #[test]
    fn dispatcher_cannot_delete_or_publish() {
        assert!(!has_permission(ROLE_DISPATCHER, Permission::OpsRoutesDelete));
        assert!(!has_permission(ROLE_DISPATCHER, Permission::OpsTripsDelete));
        assert!(!has_permission(ROLE_DISPATCHER, Permission::OpsConfigPublish));
        assert!(!has_permission(ROLE_DISPATCHER, Permission::OpsConfigWrite));
    }

    #[test]
    fn staff_user_is_read_only() {
        let write_perms = [
            Permission::OpsRoutesWrite, Permission::OpsTripsWrite,
            Permission::PaymentsTransactionsWrite, Permission::UsersWrite,
        ];
        for p in write_perms {
            assert!(!has_permission(ROLE_STAFF_USER, p), "staff_user should not have {:?}", p);
        }
    }

    #[test]
    fn finance_analyst_cannot_access_ops_write() {
        assert!(!has_permission(ROLE_FINANCE_ANALYST, Permission::OpsRoutesWrite));
        assert!(!has_permission(ROLE_FINANCE_ANALYST, Permission::OpsTripsWrite));
        assert!(!has_permission(ROLE_FINANCE_ANALYST, Permission::AuditRead));
    }

    #[test]
    fn no_role_has_roles_write() {
        for role in [ROLE_OPERATIONS_ADMIN, ROLE_DISPATCHER, ROLE_FINANCE_ANALYST, ROLE_STAFF_USER] {
            assert!(!has_permission(role, Permission::RolesWrite), "{} should not have RolesWrite", role);
        }
    }

    #[test]
    fn unknown_role_denied() {
        assert!(!has_permission("unknown_role", Permission::ReportingRead));
    }
}
