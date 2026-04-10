use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================
// Request bodies
// ============================================================

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct ReauthRequest {
    pub password: String,
}

// ============================================================
// Response bodies
// ============================================================

#[derive(Serialize)]
pub struct LoginResponse {
    /// Raw session token — client stores this and sends as `Authorization: Bearer <token>`.
    pub token:      String,
    pub expires_at: DateTime<Utc>,
    pub user_id:    Uuid,
    pub username:   String,
    pub role:       String,
}

#[derive(Serialize)]
pub struct SessionResponse {
    pub session_id:        Uuid,
    pub user_id:           Uuid,
    pub username:          String,
    pub role:              String,
    pub expires_at:        DateTime<Utc>,
    pub last_activity_at:  DateTime<Utc>,
    /// True when `last_reauth_at` is absent or outside the reauth window.
    /// Front-end uses this to decide whether to prompt for re-authentication.
    pub reauth_required:   bool,
}

#[derive(Serialize)]
pub struct ReauthResponse {
    pub message:   &'static str,
    pub reauth_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct LogoutResponse {
    pub message: &'static str,
}

// ============================================================
// Internal DB row types (not serialised to callers)
// ============================================================

/// Returned by the login query (users JOIN roles).
#[derive(sqlx::FromRow)]
pub struct UserRow {
    pub id:                     Uuid,
    pub username:               String,
    pub password_hash:          String,
    pub role_name:              String,
    pub is_active:              bool,
    pub failed_login_attempts:  i32,
    pub locked_until:           Option<DateTime<Utc>>,
}

/// Returned by the session-validation query (sessions JOIN users JOIN roles).
#[derive(sqlx::FromRow, Clone)]
pub struct SessionRow {
    pub id:               Uuid,
    pub user_id:          Uuid,
    pub expires_at:       DateTime<Utc>,
    pub revoked_at:       Option<DateTime<Utc>>,
    pub last_activity_at: DateTime<Utc>,
    pub last_reauth_at:   Option<DateTime<Utc>>,
    pub username:         String,
    pub role_name:        String,
}
