/// `ScopeGuard` — Actix-web Transform middleware for scope-level permission enforcement.
///
/// Validates the Bearer token AND checks a single required `Permission` before
/// allowing the request to reach the inner service.  On success, the resolved
/// `AuthSession` is stored in request extensions so downstream `AuthSession`
/// extractors skip a second DB round-trip.
///
/// Usage:
/// ```rust
/// web::scope("/admin/audit")
///     .wrap(ScopeGuard::require(Permission::AuditRead))
///     .route("", web::get().to(audit::list_logs))
/// ```
use std::{
    future::{ready, Ready},
    rc::Rc,
};

use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::StatusCode,
    web, HttpResponse, ResponseError,
};
use futures_util::future::LocalBoxFuture;
use sha2::{Digest, Sha256};

use crate::{
    auth::middleware::{fetch_validated_session, AuthSession},
    rbac::permissions::{has_permission, Permission},
    AppState,
};

// ============================================================
// Public API
// ============================================================

/// Middleware factory.  Constructed via [`ScopeGuard::require`].
pub struct ScopeGuard {
    permission: Permission,
}

impl ScopeGuard {
    pub fn require(permission: Permission) -> Self {
        Self { permission }
    }
}

// ============================================================
// Transform impl — called once per worker thread at startup
// ============================================================

impl<S, B> Transform<S, ServiceRequest> for ScopeGuard
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response   = ServiceResponse<EitherBody<B>>;
    type Error      = actix_web::Error;
    type InitError  = ();
    type Transform  = ScopeGuardMiddleware<S>;
    type Future     = Ready<Result<Self::Transform, ()>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ScopeGuardMiddleware {
            service:    Rc::new(service),
            permission: self.permission,
        }))
    }
}

// ============================================================
// Service impl — called on every request
// ============================================================

pub struct ScopeGuardMiddleware<S> {
    service:    Rc<S>,
    permission: Permission,
}

impl<S, B> Service<ServiceRequest> for ScopeGuardMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error    = actix_web::Error;
    type Future   = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service    = Rc::clone(&self.service);
        let permission = self.permission;

        Box::pin(async move {
            // Phase 1: validate token + check permission (immutable borrow of req)
            let auth_result = check_permission(&req, permission).await;

            match auth_result {
                Ok(session) => {
                    // Phase 2: cache session so AuthSession extractor skips DB
                    req.extensions_mut().insert(session);
                    service.call(req).await.map(|res| res.map_into_left_body())
                }
                Err(err_resp) => {
                    // Short-circuit with error response
                    Ok(req.into_response(err_resp).map_into_right_body())
                }
            }
        })
    }
}

// ============================================================
// Core check logic (borrows ServiceRequest immutably)
// ============================================================

/// Returns the validated `AuthSession` if the bearer token is valid and the
/// resolved role holds `permission`.  Returns a ready `HttpResponse` on any
/// failure so the caller can short-circuit without moving `req`.
async fn check_permission(
    req: &ServiceRequest,
    permission: Permission,
) -> Result<AuthSession, HttpResponse> {
    // 1. Extract bearer token
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::to_string)
        .ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "Missing Authorization header", "UNAUTHORIZED"))?;

    // 2. Get app state
    let state = req
        .app_data::<web::Data<AppState>>()
        .cloned()
        .ok_or_else(|| error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error", "INTERNAL_ERROR"))?;

    // 3. Validate session (checks revocation, expiry, inactivity; updates last_activity_at)
    let token_hash = hash_token(&token);
    let session = fetch_validated_session(&state.db, &state.config, &token_hash)
        .await
        .map_err(|app_err| app_err.error_response())?;

    // 4. Permission check against static policy
    if !has_permission(&session.role, permission) {
        return Err(error_response(
            StatusCode::FORBIDDEN,
            &format!(
                "Role '{}' does not have permission '{}'",
                session.role, permission
            ),
            "FORBIDDEN",
        ));
    }

    Ok(session)
}

// ============================================================
// Private helpers
// ============================================================

fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}

fn error_response(status: StatusCode, message: &str, code: &str) -> HttpResponse {
    HttpResponse::build(status).json(serde_json::json!({
        "error": message,
        "code":  code
    }))
}
