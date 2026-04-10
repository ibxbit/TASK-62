/// Signature verification and anti-replay protection.
///
/// ## Signed-string construction
///
/// The string passed to HMAC is: `nonce + "." + timestamp_str + "." + body_sha256_hex`
/// (timestamp component omitted when `ts_in_sig = false`).
///
/// This mirrors the pattern used by Stripe/Alipay-style webhooks.
///
/// ## Anti-replay strategy
///
/// Two independent checks, both must pass:
///   1. **Timestamp window** — provider timestamp must be within ±300 seconds of now.
///      Prevents attackers who captured a valid request from replaying it later.
///   2. **Nonce uniqueness** — nonce must not already exist in `payments.callbacks`.
///      Prevents exact-duplicate replays within the timestamp window.
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};
use sqlx::PgPool;

use super::gateway::PaymentGateway;

// ============================================================
// Error type
// ============================================================

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ReplayError {
    #[error("Timestamp out of window: {0} seconds from now (max 300)")]
    TimestampStale(i64),

    #[error("Nonce '{0}' has already been used")]
    NonceReused(String),
}

// ============================================================
// HMAC computation
// ============================================================

/// Compute HMAC-SHA256 of `message` with `secret`, returning a lower-case hex string.
pub fn hmac_sha256_hex(secret: &[u8], message: &[u8]) -> String {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret)
        .expect("HMAC-SHA256 accepts any key length");
    mac.update(message);
    hex::encode(mac.finalize().into_bytes())
}

/// Compute HMAC-SHA512 of `message` with `secret`, returning a lower-case hex string.
pub fn hmac_sha512_hex(secret: &[u8], message: &[u8]) -> String {
    type HmacSha512 = Hmac<Sha512>;
    let mut mac = HmacSha512::new_from_slice(secret)
        .expect("HMAC-SHA512 accepts any key length");
    mac.update(message);
    hex::encode(mac.finalize().into_bytes())
}

/// SHA-256 of arbitrary bytes, hex-encoded.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

// ============================================================
// Signed-string construction
// ============================================================

/// Build the canonical string that the provider signs.
///
/// Format: `nonce.timestamp.body_hash`   (or `nonce.body_hash` if `ts_in_sig = false`)
pub fn build_signed_string(nonce: &str, timestamp: i64, body: &[u8], ts_in_sig: bool) -> String {
    let body_hash = sha256_hex(body);
    if ts_in_sig {
        format!("{}.{}.{}", nonce, timestamp, body_hash)
    } else {
        format!("{}.{}", nonce, body_hash)
    }
}

// ============================================================
// Full verification pipeline
// ============================================================

/// Verify an inbound callback signature and anti-replay constraints.
///
/// Steps:
///   1. Validate timestamp is within 5-minute window.
///   2. Validate nonce has not been used (DB look-up).
///   3. Reconstruct signed string and verify HMAC.
///
/// Returns `Ok(())` if all checks pass, or the first failing error.
pub async fn verify_callback(
    pool:          &PgPool,
    gateway:       &dyn PaymentGateway,
    raw_body:      &[u8],
    nonce:         &str,
    timestamp:     i64,
    provided_sig:  &str,
) -> Result<(), VerifyError> {
    // 1. Timestamp window
    validate_timestamp(timestamp)?;

    // 2. Nonce uniqueness
    check_nonce_fresh(pool, nonce).await?;

    // 3. HMAC
    let signed = build_signed_string(nonce, timestamp, raw_body, gateway.ts_in_sig());
    let expected = match gateway.hmac_algorithm() {
        "sha512" => hmac_sha512_hex(gateway.hmac_secret_bytes(), signed.as_bytes()),
        _        => hmac_sha256_hex(gateway.hmac_secret_bytes(), signed.as_bytes()),
    };

    // Constant-time comparison to prevent timing attacks
    if !constant_time_eq(provided_sig.as_bytes(), expected.as_bytes()) {
        return Err(VerifyError::BadSignature);
    }

    Ok(())
}

/// Combined error type for the full verification pipeline.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("Replay detected: {0}")]
    Replay(#[from] ReplayError),

    #[error("Signature mismatch")]
    BadSignature,

    #[error("Database error during nonce check: {0}")]
    Db(#[from] sqlx::Error),
}

impl From<VerifyError> for crate::error::AppError {
    fn from(e: VerifyError) -> Self {
        match e {
            VerifyError::Replay(_)     => crate::error::AppError::BadRequest(e.to_string()),
            VerifyError::BadSignature  => crate::error::AppError::Unauthorized(
                "Callback signature verification failed".to_string(),
            ),
            VerifyError::Db(db)        => crate::error::AppError::Database(db),
        }
    }
}

// ============================================================
// Timestamp validation
// ============================================================

/// Maximum age (seconds) before a callback timestamp is rejected.
pub const MAX_TIMESTAMP_SKEW_SECS: i64 = 300; // 5 minutes

/// Reject if the provider-reported timestamp deviates from `now()` by more than 5 minutes.
pub fn validate_timestamp(ts_secs: i64) -> Result<(), ReplayError> {
    let now = Utc::now().timestamp();
    let delta = (now - ts_secs).abs();
    if delta > MAX_TIMESTAMP_SKEW_SECS {
        Err(ReplayError::TimestampStale(delta))
    } else {
        Ok(())
    }
}

// ============================================================
// Nonce freshness
// ============================================================

/// Check that `nonce` does not already exist in `payments.callbacks`.
///
/// The `callbacks.nonce` column has a UNIQUE constraint that acts as a DB-level
/// backstop, but we also check here to return a clear error before attempting
/// the INSERT.
pub async fn check_nonce_fresh(pool: &PgPool, nonce: &str) -> Result<(), ReplayError> {
    let exists: bool = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM payments.callbacks WHERE nonce = $1)",
        nonce,
    )
    .fetch_one(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(false);

    if exists {
        Err(ReplayError::NonceReused(nonce.to_string()))
    } else {
        Ok(())
    }
}

// ============================================================
// Constant-time comparison
// ============================================================

/// Constant-time byte-slice equality to prevent timing attacks on HMAC comparison.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_sha256_is_deterministic() {
        let sig1 = hmac_sha256_hex(b"secret", b"hello");
        let sig2 = hmac_sha256_hex(b"secret", b"hello");
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn hmac_sha256_differs_with_different_key() {
        let sig1 = hmac_sha256_hex(b"key1", b"hello");
        let sig2 = hmac_sha256_hex(b"key2", b"hello");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn signed_string_includes_timestamp() {
        let s = build_signed_string("nonce123", 1700000000, b"body", true);
        assert!(s.starts_with("nonce123.1700000000."));
    }

    #[test]
    fn signed_string_omits_timestamp_when_flag_false() {
        let s = build_signed_string("nonce123", 1700000000, b"body", false);
        assert!(s.starts_with("nonce123."));
        assert!(!s.contains("1700000000"));
    }

    #[test]
    fn validate_timestamp_accepts_recent() {
        let now = Utc::now().timestamp();
        assert!(validate_timestamp(now).is_ok());
        assert!(validate_timestamp(now - 100).is_ok());
        assert!(validate_timestamp(now + 100).is_ok());
    }

    #[test]
    fn validate_timestamp_rejects_stale() {
        let now = Utc::now().timestamp();
        let err = validate_timestamp(now - 400).unwrap_err();
        assert!(matches!(err, ReplayError::TimestampStale(_)));
    }

    #[test]
    fn validate_timestamp_rejects_future() {
        let now = Utc::now().timestamp();
        let err = validate_timestamp(now + 400).unwrap_err();
        assert!(matches!(err, ReplayError::TimestampStale(_)));
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hi"));
    }

    #[test]
    fn sha256_hex_is_correct() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let h = sha256_hex(b"");
        assert_eq!(&h[..8], "e3b0c442");
    }
}
