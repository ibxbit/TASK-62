use serde::{Deserialize, Serialize};

/// Session information returned by GET /auth/session and POST /auth/login.
#[derive(Clone, PartialEq, Deserialize, Default, Debug)]
pub struct SessionInfo {
    pub username:   String,
    pub role:       String,
    pub session_id: Option<String>,
}

impl SessionInfo {
    pub fn is_admin(&self) -> bool      { self.role == "operations_admin" }
    pub fn is_dispatcher(&self) -> bool { self.role == "dispatcher" }
    pub fn is_finance(&self) -> bool    { self.role == "finance_analyst" }
    pub fn is_staff(&self) -> bool      { self.role == "staff_user" }

    /// Returns true for roles that can access the ops config area.
    pub fn can_ops_config(&self) -> bool {
        matches!(self.role.as_str(), "operations_admin" | "dispatcher" | "finance_analyst" | "staff_user")
    }
    /// Returns true for roles that can publish configs (admin only).
    pub fn can_publish(&self) -> bool { self.is_admin() }
    /// Returns true for roles that can access finance/reconciliation.
    pub fn can_finance(&self) -> bool { self.is_admin() || self.is_finance() }
    /// Returns true for roles that can access reporting.
    pub fn can_reporting(&self) -> bool {
        matches!(self.role.as_str(), "operations_admin" | "dispatcher" | "finance_analyst" | "staff_user")
    }
    /// Returns true for roles that can manage metrics (create/update/delete).
    pub fn can_manage_metrics(&self) -> bool { self.is_admin() || self.is_finance() }
    /// Returns true for roles that can view alerts.
    pub fn can_alerts(&self) -> bool {
        matches!(self.role.as_str(), "operations_admin" | "dispatcher" | "finance_analyst")
    }
}

/// Body for POST /auth/login.
#[derive(Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Response from POST /auth/login.
#[derive(Deserialize)]
pub struct LoginResponse {
    pub token:    String,
    pub username: String,
    pub role:     String,
}

/// Body for POST /auth/reauth.
#[derive(Serialize)]
pub struct ReauthRequest {
    pub password: String,
}
