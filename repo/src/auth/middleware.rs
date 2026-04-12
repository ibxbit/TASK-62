use actix_web::HttpMessage;
/// Session validation via Actix-web `FromRequest` extractors.
///
/// Two extractors are provided:
///
/// `AuthSession`   — validates the Bearer token, enforces inactivity timeout,
///                   refreshes `last_activity_at`.  Checks the request-extension
///                   cache first so `ScopeGuard` middleware (which runs before
///                   the handler) never causes a second DB round-trip.
///
/// `ReauthGuard`   — wraps `AuthSession`; additionally requires `last_reauth_at`
///                   to be within `config.reauth_window_minutes`.
///
/// `AuthSession::require(permission)` — handler-level permission check.
///
/// Token scheme:
///   client  →  Authorization: Bearer <64-char-hex raw token>
///   server  →  stores SHA-256(raw_token) as token_hash — raw token never on disk.
use std::{future::Future, pin::Pin};

use actix_web::{dev::Payload, web, FromRequest, HttpRequest};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::models::SessionRow,
    config::Config,
    error::AppError,
    rbac::permissions::{has_permission, Permission},
    AppState,
};

// ============================================================
// AuthSession — validated session context attached to a request
// ============================================================

#[derive(Clone)]
pub struct AuthSession {
    pub session_id:       Uuid,
    pub user_id:          Uuid,
    pub username:         String,
    pub role:             String,
    pub expires_at:       DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub last_reauth_at:   Option<DateTime<Utc>>,
}

impl AuthSession {
    /// Handler-level permission gate.
    ///
    /// ```rust
    /// async fn update_route(session: AuthSession, ...) -> Result<...> {
    ///     session.require(Permission::OpsRoutesWrite)?;
    ///     ...
    /// }
    /// ```
    pub fn require(&self, permission: Permission) -> Result<(), AppError> {
        if has_permission(&self.role, permission) {
            Ok(())
        } else {
            Err(AppError::Forbidden(format!(
                "Role '{}' does not have permission '{}'",
                self.role, permission
            )))
        }
    }
}

impl FromRequest for AuthSession {
    type Error  = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let req = req.clone();
        Box::pin(async move {
            // ----------------------------------------------------------
            // Fast path: ScopeGuard already validated & cached the session
            // ----------------------------------------------------------
            if let Some(cached) = req.extensions().get::<AuthSession>() {
                return Ok(cached.clone());
            }

            // ----------------------------------------------------------
            // Slow path: first authenticated call on this request
            // ----------------------------------------------------------
            let state = req
                .app_data::<web::Data<AppState>>()
                .cloned()
                .ok_or_else(|| AppError::Internal("App state not configured".to_string()))?;

            let token      = extract_bearer_token(&req)?;
            let token_hash = hash_token(&token);

            let session =
                fetch_validated_session(&state.db, &state.config, &token_hash).await?;

            // Cache for any subsequent extractor calls on the same request
            req.extensions_mut().insert(session.clone());

            Ok(session)
        })
    }
}

// ============================================================
// ReauthGuard — AuthSession + recent re-auth verification
// ============================================================

pub struct ReauthGuard(pub AuthSession);

impl FromRequest for ReauthGuard {
    type Error  = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let session_future = AuthSession::from_request(req, payload);
        let req = req.clone();
        Box::pin(async move {
            let session = session_future.await?;

            let state = req
                .app_data::<web::Data<AppState>>()
                .cloned()
                .ok_or_else(|| AppError::Internal("App state not configured".to_string()))?;

            let window = Duration::minutes(state.config.reauth_window_minutes);

            match session.last_reauth_at {
                Some(t) if Utc::now() - t <= window => Ok(ReauthGuard(session)),
                _ => Err(AppError::Forbidden(
                    "This action requires re-authentication within the last 10 minutes. \
                     POST /auth/reauth to continue."
                        .to_string(),
                )),
            }
        })
    }
}

// ============================================================
// Core session validation  (shared by FromRequest + ScopeGuard)
// ============================================================

/// Loads, validates, and refreshes a session from the DB using a pre-computed
/// `token_hash`.  This is the single authoritative validation path — both the
/// `AuthSession` extractor and `ScopeGuard` middleware call this function.
///
/// Checks performed (in order):
///   1. Token exists in DB (not found → 401)
///   2. Session not revoked  (→ 401)
///   3. Absolute expiry      (→ 401)
///   4. Inactivity timeout   (30 min idle → revoke + 401)
///   5. Updates `last_activity_at` on every valid call
pub(crate) async fn fetch_validated_session(
    pool:       &PgPool,
    config:     &Config,
    token_hash: &str,
) -> Result<AuthSession, AppError> {
    // 1. Load session row + user context in a single JOIN
    let row = sqlx::query_as::<_, SessionRow>(
        r#"
        SELECT s.id,
               s.user_id,
               s.expires_at,
               s.revoked_at,
               s.last_activity_at,
               s.last_reauth_at,
               u.username,
               r.name AS role_name
        FROM   auth.sessions s
        JOIN   auth.users u ON u.id  = s.user_id
        JOIN   auth.roles  r ON r.id = u.role_id
        WHERE  s.token_hash   = $1
          AND  u.deleted_at   IS NULL
          AND  u.is_active    = TRUE
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::Unauthorized("Invalid or expired token".to_string()))?;

    // 2. Explicit revocation
    if row.revoked_at.is_some() {
        return Err(AppError::Unauthorized("Session has been revoked".to_string()));
    }

    // 3. Absolute expiry
    if row.expires_at < Utc::now() {
        return Err(AppError::Unauthorized("Session has expired".to_string()));
    }

    // 4. Inactivity timeout — revoke & reject
    let idle_limit = Duration::minutes(config.inactivity_timeout_minutes);
    if Utc::now() - row.last_activity_at > idle_limit {
        let _ = sqlx::query(
            "UPDATE auth.sessions SET revoked_at = now() WHERE id = $1",
        )
        .bind(row.id)
        .execute(pool)
        .await;

        return Err(AppError::Unauthorized(
            "Session expired due to inactivity. Please log in again.".to_string(),
        ));
    }

    // 5. Refresh activity timestamp (non-fatal if it fails)
    let _ = sqlx::query(
        "UPDATE auth.sessions SET last_activity_at = now() WHERE id = $1",
    )
    .bind(row.id)
    .execute(pool)
    .await;

    Ok(AuthSession {
        session_id:       row.id,
        user_id:          row.user_id,
        username:         row.username,
        role:             row.role_name,
        expires_at:       row.expires_at,
        last_activity_at: Utc::now(),
        last_reauth_at:   row.last_reauth_at,
    })
}

// ============================================================
// Shared helpers  (pub(crate) so handlers.rs can import them)
// ============================================================

/// Extract the raw token string from `Authorization: Bearer <token>`.
pub(crate) fn extract_bearer_token(req: &HttpRequest) -> Result<String, AppError> {
    let header = req
        .headers()
        .get("Authorization")
        .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?
        .to_str()
        .map_err(|_| AppError::Unauthorized("Invalid Authorization header encoding".to_string()))?;

    header
        .strip_prefix("Bearer ")
        .map(str::to_string)
        .ok_or_else(|| AppError::Unauthorized("Authorization scheme must be Bearer".to_string()))
}

/// SHA-256 of the raw token, hex-encoded.  Stored in DB; raw token never persisted.
pub(crate) fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}
