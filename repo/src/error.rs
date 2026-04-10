use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error")]
    Database(#[from] sqlx::Error),

    /// 401 — caller is not authenticated or token is invalid.
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// 403 — caller is authenticated but lacks permission (e.g. reauth required).
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// 400 — malformed request body or parameters.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// 404 — requested resource does not exist.
    #[error("Not found: {0}")]
    NotFound(String),

    /// 429 — account is locked due to too many failed attempts.
    #[error("Account locked until {0}")]
    AccountLocked(String),

    /// 500 — unexpected server-side failure.
    #[error("Internal server error")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    code:  &'static str,
}

impl AppError {
    fn error_code(&self) -> &'static str {
        match self {
            AppError::Database(_) | AppError::Internal(_) => "INTERNAL_ERROR",
            AppError::Unauthorized(_)                     => "UNAUTHORIZED",
            AppError::Forbidden(_)                        => "FORBIDDEN",
            AppError::BadRequest(_)                       => "BAD_REQUEST",
            AppError::NotFound(_)                         => "NOT_FOUND",
            AppError::AccountLocked(_)                    => "ACCOUNT_LOCKED",
        }
    }
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Database(_) | AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Unauthorized(_)                     => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_)                        => StatusCode::FORBIDDEN,
            AppError::BadRequest(_)                       => StatusCode::BAD_REQUEST,
            AppError::NotFound(_)                         => StatusCode::NOT_FOUND,
            AppError::AccountLocked(_)                    => StatusCode::TOO_MANY_REQUESTS,
        }
    }

    fn error_response(&self) -> HttpResponse {
        // Log server-side errors; surface nothing to the caller.
        if let AppError::Database(ref e) = self {
            tracing::error!("Database error: {:?}", e);
        }
        if let AppError::Internal(ref msg) = self {
            tracing::error!("Internal error: {}", msg);
        }
        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: self.to_string(),
            code:  self.error_code(),
        })
    }
}

impl From<crate::crypto::CryptoError> for AppError {
    fn from(e: crate::crypto::CryptoError) -> Self {
        // Never leak crypto internals to the caller.
        tracing::error!("crypto error: {}", e);
        AppError::Internal("Cryptographic operation failed".to_string())
    }
}

impl From<argon2::password_hash::Error> for AppError {
    fn from(_err: argon2::password_hash::Error) -> Self {
        // Never leak hash internals to caller.
        AppError::Internal("Password processing error".to_string())
    }
}
