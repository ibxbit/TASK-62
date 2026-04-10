pub mod middleware;
pub mod permissions;

// Re-export the most common items for ergonomic use in handler modules.
pub use middleware::ScopeGuard;
pub use permissions::{has_permission, Permission};
pub use permissions::{
    ROLE_DISPATCHER, ROLE_FINANCE_ANALYST, ROLE_OPERATIONS_ADMIN, ROLE_STAFF_USER,
};
