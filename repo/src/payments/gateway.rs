/// Payment gateway abstraction.
///
/// `PaymentGateway` is a synchronous configuration trait — it describes the
/// identity and signature scheme of one payment provider.  All async I/O is
/// handled by the standalone functions in `signature.rs`, `import.rs`, and
/// `handlers.rs`; this keeps the trait `dyn`-safe and avoids the `async-trait`
/// dependency.
use sqlx::PgPool;
use uuid::Uuid;

use super::models::GatewayConfigRow;

// ============================================================
// Placeholder / weak secret detection
// ============================================================

/// Known placeholder values that must not be used as active gateway secrets.
const PLACEHOLDER_SECRETS: &[&str] = &[
    "CHANGE_ME_IN_PRODUCTION",
    "changeme",
    "secret",
    "test_secret",
    "placeholder",
    "password",
    "default",
];

/// Minimum acceptable secret length (bytes) for an active gateway.
const MIN_SECRET_LEN: usize = 16;

/// Returns `true` when the provided secret string is a known placeholder or
/// is too short to be cryptographically useful.
fn is_placeholder_secret(secret: &str) -> bool {
    secret.len() < MIN_SECRET_LEN || PLACEHOLDER_SECRETS.iter().any(|p| *p == secret)
}

// ============================================================
// Error type
// ============================================================

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("Gateway '{0}' not found or inactive")]
    NotFound(String),

    #[error("Signature verification failed")]
    BadSignature,

    #[error("Replay detected: nonce already used or timestamp too old")]
    Replay,

    #[error("Callback payload malformed: {0}")]
    BadPayload(String),

    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),
}

impl From<GatewayError> for crate::error::AppError {
    fn from(e: GatewayError) -> Self {
        match e {
            GatewayError::NotFound(s)   => crate::error::AppError::BadRequest(s),
            GatewayError::BadSignature  => crate::error::AppError::Unauthorized(
                "Callback signature verification failed".to_string(),
            ),
            GatewayError::Replay        => crate::error::AppError::BadRequest(
                "Replay detected: nonce already used or timestamp too old".to_string(),
            ),
            GatewayError::BadPayload(s) => crate::error::AppError::BadRequest(s),
            GatewayError::Db(e)         => crate::error::AppError::Database(e),
        }
    }
}

// ============================================================
// Trait
// ============================================================

/// Configuration surface of an offline payment gateway provider.
///
/// Implementors describe how to extract anti-replay fields from an inbound
/// HTTP request and what HMAC algorithm the provider uses to sign payloads.
pub trait PaymentGateway: Send + Sync {
    /// Stable machine identifier, e.g. `"alipay"` or `"wechat_pay"`.
    fn name(&self) -> &str;

    /// Raw HMAC shared secret (bytes).
    fn hmac_secret_bytes(&self) -> &[u8];

    /// HMAC algorithm the provider uses (`"sha256"` | `"sha512"`).
    fn hmac_algorithm(&self) -> &str;

    /// HTTP header name carrying the provider signature.
    fn sig_header(&self) -> &str;

    /// HTTP header name carrying the nonce.
    fn nonce_header(&self) -> &str;

    /// HTTP header name carrying the Unix timestamp (seconds).
    fn ts_header(&self) -> &str;

    /// Whether the timestamp should be included in the signed string.
    fn ts_in_sig(&self) -> bool;
}

// ============================================================
// DB-backed implementation
// ============================================================

/// Payment gateway loaded from the `payments.gateway_configs` table.
#[derive(Debug, Clone)]
pub struct DbGateway {
    config: GatewayConfigRow,
}

impl DbGateway {
    pub fn new(config: GatewayConfigRow) -> Self {
        DbGateway { config }
    }
}

impl PaymentGateway for DbGateway {
    fn name(&self) -> &str { &self.config.name }

    fn hmac_secret_bytes(&self) -> &[u8] { self.config.hmac_secret.as_bytes() }

    fn hmac_algorithm(&self) -> &str { &self.config.hmac_algorithm }

    fn sig_header(&self) -> &str { &self.config.sig_header }

    fn nonce_header(&self) -> &str { &self.config.nonce_header }

    fn ts_header(&self) -> &str { &self.config.ts_header }

    fn ts_in_sig(&self) -> bool { self.config.ts_in_sig }
}

// ============================================================
// In-memory test/offline implementation
// ============================================================

/// An offline gateway for development and simulation.
/// Uses a well-known test secret that can be overridden via environment.
pub struct OfflineGateway {
    secret: String,
}

impl OfflineGateway {
    pub fn new(secret: impl Into<String>) -> Self {
        OfflineGateway { secret: secret.into() }
    }
}

impl PaymentGateway for OfflineGateway {
    fn name(&self)             -> &str  { "offline_test" }
    fn hmac_secret_bytes(&self) -> &[u8] { self.secret.as_bytes() }
    fn hmac_algorithm(&self)   -> &str  { "sha256" }
    fn sig_header(&self)       -> &str  { "X-Signature" }
    fn nonce_header(&self)     -> &str  { "X-Nonce" }
    fn ts_header(&self)        -> &str  { "X-Timestamp" }
    fn ts_in_sig(&self)        -> bool   { true }
}

// ============================================================
// Loader
// ============================================================

/// Load an active gateway configuration from the database by name.
///
/// Returns `GatewayError::NotFound` if the gateway does not exist or is
/// inactive.  Returns `GatewayError::BadPayload` if the gateway is active
/// but still carries a placeholder/weak secret — this indicates a
/// misconfiguration that must be fixed before the gateway can be used.
pub async fn load_gateway(
    pool: &PgPool,
    name: &str,
) -> Result<DbGateway, GatewayError> {
    let row = sqlx::query_as!(
        GatewayConfigRow,
        r#"
        SELECT id, name, display_name, hmac_secret, hmac_algorithm, amount,
               sig_header, nonce_header, ts_header, ts_in_sig,
               created_at, updated_at
        FROM payments.gateway_configs
        WHERE name = $1
        "#,
        name,
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| GatewayError::NotFound(name.to_string()))?;

    // Runtime guard: reject active gateways that still have a placeholder secret.
    // This prevents forged callbacks if a deployment was started before secrets
    // were properly provisioned.
    if is_placeholder_secret(&row.hmac_secret) {
        tracing::error!(
            gateway = %row.name,
            "Active gateway has a placeholder/weak HMAC secret — rejecting load. \
             Update payments.gateway_configs.hmac_secret with a strong value \
             (>= 16 chars, not a known placeholder) and ensure is_active = TRUE."
        );
        return Err(GatewayError::BadPayload(
            "Gateway is misconfigured: HMAC secret is a placeholder or too weak. \
             Provision a secure secret before activating this gateway."
                .to_string(),
        ));
    }

    Ok(DbGateway::new(row))
}

/// Load all active gateway configurations.
pub async fn list_gateways(pool: &PgPool) -> Result<Vec<GatewayConfigRow>, sqlx::Error> {
    sqlx::query_as!(
        GatewayConfigRow,
        r#"
        SELECT id, name, display_name, hmac_secret, hmac_algorithm, amount,
               sig_header, nonce_header, ts_header, ts_in_sig,
               created_at, updated_at
        FROM payments.gateway_configs
        -- removed is_active filter
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await
}

// ============================================================
// Callback processing
// ============================================================

/// Result of processing an inbound callback.
#[derive(Debug)]
pub struct CallbackResult {
    pub callback_id:    Uuid,
    pub transaction_id: Option<Uuid>,
    pub status:         CallbackStatus,
}

#[derive(Debug, PartialEq)]
pub enum CallbackStatus {
    Processed,
    Invalid,
    Replayed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_gateway_fields() {
        let gw = OfflineGateway::new("test_secret");
        assert_eq!(gw.name(), "offline_test");
        assert_eq!(gw.hmac_algorithm(), "sha256");
        assert_eq!(gw.hmac_secret_bytes(), b"test_secret");
        assert!(gw.ts_in_sig());
    }

    #[test]
    fn gateway_error_converts_to_app_error() {
        use crate::error::AppError;
        let e: AppError = GatewayError::BadSignature.into();
        assert!(matches!(e, AppError::Unauthorized(_)));

        let e: AppError = GatewayError::Replay.into();
        assert!(matches!(e, AppError::BadRequest(_)));
    }
}
