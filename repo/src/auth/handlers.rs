use actix_web::{web, HttpRequest, HttpResponse};
use chrono::{Duration, Utc};
use rand::RngCore;
use uuid::Uuid;

use crate::{
    auth::{
        middleware::{hash_token, AuthSession},
        models::{
            LoginRequest, LoginResponse, LogoutResponse, ReauthRequest, ReauthResponse,
            SessionResponse, UserRow,
        },
        password::verify_password,
    },
    error::AppError,
    AppState,
};

// ============================================================
// POST /auth/login
// ============================================================
//
// Flow:
//  1. Load user row (returns same error for unknown user or wrong password
//     to prevent user enumeration)
//  2. Check account lockout
//  3. Verify password with argon2id (inherently ~100 ms — natural brute-force delay)
//  4. On failure: increment failed_attempts; lock account after threshold is reached
//  5. On success: reset failed_attempts, create session, return raw token
// ============================================================

pub async fn login(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, AppError> {
    let ip = req
        .connection_info()
        .realip_remote_addr()
        .map(str::to_string);
    let user_agent = req
        .headers()
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    // --- 1. Fetch user (constant-error on not-found prevents enumeration) ---
    let user = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT u.id,
               u.username,
               u.password_hash,
               u.is_active,
               u.failed_login_attempts,
               u.locked_until,
               r.name AS role_name
        FROM   auth.users u
        JOIN   auth.roles r ON r.id = u.role_id
        WHERE  u.username  = $1
          AND  u.deleted_at IS NULL
        "#,
    )
    .bind(&body.username)
    .fetch_optional(&state.db)
    .await?;

    // Treat "user not found" identically to "wrong password" at the HTTP layer.
    let user = match user {
        Some(u) => u,
        None    => return Err(AppError::Unauthorized("Invalid credentials".to_string())),
    };

    // --- 2. Lockout check ---
    if let Some(locked_until) = user.locked_until {
        if locked_until > Utc::now() {
            return Err(AppError::AccountLocked(locked_until.to_rfc3339()));
        }
    }

    if !user.is_active {
        return Err(AppError::Unauthorized("Account is inactive".to_string()));
    }

    // --- 3. Verify password (argon2id; constant-time comparison inside crate) ---
    let valid = verify_password(&body.password, &user.password_hash)?;

    if !valid {
        // --- 4. Record failed attempt + conditional lock ---
        record_failed_attempt(&state, user.id).await?;
        return Err(AppError::Unauthorized("Invalid credentials".to_string()));
    }

    // --- 5a. Reset failure counters on successful authentication ---
    sqlx::query(
        r#"
        UPDATE auth.users
        SET    failed_login_attempts = 0,
               locked_until          = NULL,
               last_login_at         = now(),
               updated_at            = now()
        WHERE  id = $1
        "#,
    )
    .bind(user.id)
    .execute(&state.db)
    .await?;

    // --- 5b. Create session ---
    let raw_token  = generate_session_token();
    let token_hash = hash_token(&raw_token);
    let session_id = Uuid::new_v4();
    let expires_at = Utc::now() + Duration::hours(state.config.session_max_age_hours);

    sqlx::query(
        r#"
        INSERT INTO auth.sessions
            (id, user_id, token_hash, issued_at, expires_at,
             last_activity_at, ip_address, user_agent)
        VALUES
            ($1, $2, $3, now(), $4, now(), $5::inet, $6)
        "#,
    )
    .bind(session_id)
    .bind(user.id)
    .bind(&token_hash)
    .bind(expires_at)
    .bind(ip.as_deref())
    .bind(user_agent.as_deref())
    .execute(&state.db)
    .await?;

    crate::audit::writer::log_auth(
        &state.db,
        user.id,
        &user.username,
        &user.role_name,
        session_id,
        crate::audit::writer::ACTION_LOGIN,
        ip.clone(),
        serde_json::json!({ "user_agent": user_agent }),
    )
    .await
    .unwrap_or_else(|e| tracing::warn!(error = %e, "audit log failed"));

    tracing::info!(
        user_id    = %user.id,
        session_id = %session_id,
        role       = %user.role_name,
        "Login successful"
    );

    Ok(HttpResponse::Ok().json(LoginResponse {
        token:      raw_token,
        expires_at,
        user_id:    user.id,
        username:   user.username,
        role:       user.role_name,
    }))
}

// ============================================================
// POST /auth/logout
// ============================================================

pub async fn logout(
    state: web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    sqlx::query(
        "UPDATE auth.sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(session.session_id)
    .execute(&state.db)
    .await?;

    crate::audit::writer::log(
        &state.db,
        &session,
        crate::audit::writer::AuditEntry {
            action:       crate::audit::writer::ACTION_LOGOUT,
            domain:       "auth",
            entity_type:  "session",
            entity_id:    session.session_id.to_string(),
            before_state: None,
            after_state:  None,
            metadata:     serde_json::Value::Object(Default::default()),
        },
    )
    .await
    .unwrap_or_else(|e| tracing::warn!(error = %e, "audit log failed"));

    tracing::info!(
        user_id    = %session.user_id,
        session_id = %session.session_id,
        "Logout"
    );

    Ok(HttpResponse::Ok().json(LogoutResponse { message: "Logged out successfully" }))
}

// ============================================================
// GET /auth/session
// ============================================================

pub async fn get_session(
    state: web::Data<AppState>,
    session: AuthSession,
) -> Result<HttpResponse, AppError> {
    let reauth_required = match session.last_reauth_at {
        Some(t) => Utc::now() - t > Duration::minutes(state.config.reauth_window_minutes),
        None    => true,
    };

    Ok(HttpResponse::Ok().json(SessionResponse {
        session_id:       session.session_id,
        user_id:          session.user_id,
        username:         session.username,
        role:             session.role,
        expires_at:       session.expires_at,
        last_activity_at: session.last_activity_at,
        reauth_required,
    }))
}

// ============================================================
// POST /auth/reauth
// ============================================================
//
// The user is already authenticated (AuthSession extractor runs first).
// We verify their password again and stamp last_reauth_at on the session.
// This satisfies the ReauthGuard check for subsequent admin-action calls.
// ============================================================

pub async fn reauth(
    state: web::Data<AppState>,
    session: AuthSession,
    body: web::Json<ReauthRequest>,
) -> Result<HttpResponse, AppError> {
    // Fetch the current password hash for this user.
    let password_hash: Option<String> = sqlx::query_scalar(
        "SELECT password_hash FROM auth.users WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await?;

    let password_hash = password_hash
        .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;

    let valid = verify_password(&body.password, &password_hash)?;
    if !valid {
        return Err(AppError::Unauthorized("Invalid credentials".to_string()));
    }

    let now = Utc::now();
    sqlx::query("UPDATE auth.sessions SET last_reauth_at = $1 WHERE id = $2")
        .bind(now)
        .bind(session.session_id)
        .execute(&state.db)
        .await?;

    tracing::info!(user_id = %session.user_id, "Re-authentication successful");

    Ok(HttpResponse::Ok().json(ReauthResponse {
        message:   "Re-authenticated successfully",
        reauth_at: now,
    }))
}

// ============================================================
// Private helpers
// ============================================================

/// 32 cryptographically random bytes, hex-encoded → 64-character token string.
fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Increment `failed_login_attempts`.  Lock the account once the threshold is hit.
async fn record_failed_attempt(state: &AppState, user_id: Uuid) -> Result<(), AppError> {
    let threshold    = state.config.lockout_threshold;
    let locked_until = Utc::now() + Duration::minutes(state.config.lockout_duration_minutes);

    sqlx::query(
        r#"
        UPDATE auth.users
        SET    failed_login_attempts = failed_login_attempts + 1,
               locked_until = CASE
                   WHEN (failed_login_attempts + 1) >= $2 THEN $3
                   ELSE locked_until
               END,
               updated_at = now()
        WHERE  id = $1
        "#,
    )
    .bind(user_id)
    .bind(threshold)
    .bind(locked_until)
    .execute(&state.db)
    .await?;

    Ok(())
}
