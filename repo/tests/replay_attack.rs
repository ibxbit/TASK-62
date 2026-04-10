//! Anti-replay and callback signature verification tests.
//!
//! All tests in this file exercise pure functions that require no database or
//! network access.  Integration scenarios requiring a live PostgreSQL connection
//! are documented as commented stubs at the bottom.
//!
//! Run: `cargo test --test replay_attack`

use chrono::Utc;
use transitops_backend::payments::signature::{
    build_signed_string, hmac_sha256_hex, hmac_sha512_hex, sha256_hex,
    validate_timestamp, MAX_TIMESTAMP_SKEW_SECS, ReplayError,
};

// ── Timestamp window ──────────────────────────────────────────────────────────

#[test]
fn timestamp_skew_window_is_five_minutes() {
    assert_eq!(MAX_TIMESTAMP_SKEW_SECS, 300);
}

#[test]
fn current_timestamp_accepted() {
    let now = Utc::now().timestamp();
    assert!(validate_timestamp(now).is_ok());
}

#[test]
fn timestamp_at_positive_boundary_accepted() {
    let now = Utc::now().timestamp();
    assert!(validate_timestamp(now - MAX_TIMESTAMP_SKEW_SECS).is_ok());
}

#[test]
fn timestamp_at_negative_boundary_accepted() {
    let now = Utc::now().timestamp();
    assert!(validate_timestamp(now + MAX_TIMESTAMP_SKEW_SECS).is_ok());
}

#[test]
fn timestamp_one_second_past_stale_rejected() {
    let now = Utc::now().timestamp();
    let err = validate_timestamp(now - MAX_TIMESTAMP_SKEW_SECS - 1).unwrap_err();
    assert!(matches!(err, ReplayError::TimestampStale(_)));
}

#[test]
fn timestamp_one_second_past_future_rejected() {
    let now = Utc::now().timestamp();
    let err = validate_timestamp(now + MAX_TIMESTAMP_SKEW_SECS + 1).unwrap_err();
    assert!(matches!(err, ReplayError::TimestampStale(_)));
}

#[test]
fn stale_timestamp_error_carries_actual_delta() {
    let now = Utc::now().timestamp();
    let ts  = now - 10_000; // ~2.8 hours ago
    match validate_timestamp(ts).unwrap_err() {
        ReplayError::TimestampStale(delta) => {
            assert!(delta > MAX_TIMESTAMP_SKEW_SECS,
                "delta {} should exceed MAX_TIMESTAMP_SKEW_SECS {}", delta, MAX_TIMESTAMP_SKEW_SECS);
        }
        other => panic!("unexpected error: {:?}", other),
    }
}

#[test]
fn unix_epoch_zero_rejected() {
    // A timestamp of 0 is always stale regardless of when tests run.
    let err = validate_timestamp(0).unwrap_err();
    assert!(matches!(err, ReplayError::TimestampStale(_)));
}

// ── Signed-string construction ────────────────────────────────────────────────

#[test]
fn signed_string_with_ts_has_three_dot_separated_parts() {
    let s = build_signed_string("nonce-abc", 1_700_000_000, b"payload", true);
    let parts: Vec<&str> = s.splitn(3, '.').collect();
    assert_eq!(parts.len(), 3, "expected nonce.timestamp.hash");
    assert_eq!(parts[0], "nonce-abc");
    assert_eq!(parts[1], "1700000000");
    assert_eq!(parts[2].len(), 64, "body hash should be 64 hex chars");
}

#[test]
fn signed_string_without_ts_has_two_dot_separated_parts() {
    let s = build_signed_string("nonce-abc", 1_700_000_000, b"payload", false);
    let parts: Vec<&str> = s.splitn(3, '.').collect();
    assert_eq!(parts.len(), 2, "expected nonce.hash only");
    assert_eq!(parts[0], "nonce-abc");
    assert!(!s.contains("1700000000"), "timestamp must not appear in signed string");
}

#[test]
fn body_change_changes_signed_string() {
    let s1 = build_signed_string("n", 100, b"original-body", true);
    let s2 = build_signed_string("n", 100, b"tampered!body", true);
    assert_ne!(s1, s2, "tampered body must produce different signed string");
}

#[test]
fn nonce_change_changes_signed_string() {
    let s1 = build_signed_string("nonce-legit",   100, b"body", true);
    let s2 = build_signed_string("nonce-attacker", 100, b"body", true);
    assert_ne!(s1, s2);
}

#[test]
fn timestamp_change_changes_signed_string_when_ts_in_sig() {
    let s1 = build_signed_string("n", 1_000, b"body", true);
    let s2 = build_signed_string("n", 2_000, b"body", true);
    assert_ne!(s1, s2);
}

/// When `ts_in_sig = false`, a replayed message with a different timestamp but
/// same nonce+body produces the SAME signed string, so the nonce check alone
/// stops the replay.
#[test]
fn timestamp_irrelevant_when_ts_not_in_sig() {
    let s1 = build_signed_string("n", 1_000, b"body", false);
    let s2 = build_signed_string("n", 2_000, b"body", false);
    assert_eq!(s1, s2, "timestamp must not affect signed string when ts_in_sig=false");
}

#[test]
fn single_byte_body_change_changes_hash() {
    // Flipping case of one character must produce a different signed string.
    let s1 = build_signed_string("n", 100, b"Amount=100.00", true);
    let s2 = build_signed_string("n", 100, b"Amount=100.01", true);
    assert_ne!(s1, s2);
}

// ── HMAC-SHA256 ───────────────────────────────────────────────────────────────

#[test]
fn hmac_sha256_is_deterministic() {
    let h1 = hmac_sha256_hex(b"key", b"message");
    let h2 = hmac_sha256_hex(b"key", b"message");
    assert_eq!(h1, h2);
}

#[test]
fn hmac_sha256_output_is_64_lowercase_hex_chars() {
    let h = hmac_sha256_hex(b"key", b"message");
    assert_eq!(h.len(), 64);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "HMAC output should be lower-case hex");
}

#[test]
fn hmac_sha256_sensitive_to_key_change() {
    let h1 = hmac_sha256_hex(b"key-a", b"msg");
    let h2 = hmac_sha256_hex(b"key-b", b"msg");
    assert_ne!(h1, h2);
}

#[test]
fn hmac_sha256_sensitive_to_message_change() {
    let h1 = hmac_sha256_hex(b"key", b"original");
    let h2 = hmac_sha256_hex(b"key", b"tampered");
    assert_ne!(h1, h2);
}

#[test]
fn hmac_sha256_empty_message_does_not_panic() {
    let _ = hmac_sha256_hex(b"key", b"");
}

// ── HMAC-SHA512 ───────────────────────────────────────────────────────────────

#[test]
fn hmac_sha512_is_deterministic() {
    assert_eq!(
        hmac_sha512_hex(b"secret", b"msg"),
        hmac_sha512_hex(b"secret", b"msg"),
    );
}

#[test]
fn hmac_sha512_output_is_128_lowercase_hex_chars() {
    let h = hmac_sha512_hex(b"key", b"message");
    assert_eq!(h.len(), 128);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
}

#[test]
fn hmac_sha256_and_sha512_produce_different_outputs() {
    // Same inputs → different digest lengths and different values.
    let h256 = hmac_sha256_hex(b"key", b"msg");
    let h512 = hmac_sha512_hex(b"key", b"msg");
    assert_ne!(h256, h512);
}

// ── SHA-256 body hash ─────────────────────────────────────────────────────────

#[test]
fn sha256_empty_body_is_well_known_constant() {
    // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb924...
    let h = sha256_hex(b"");
    assert!(h.starts_with("e3b0c442"),
        "SHA-256 of empty bytes should start with e3b0c442, got {}", &h[..8]);
    assert_eq!(h.len(), 64);
}

#[test]
fn sha256_is_deterministic() {
    let h1 = sha256_hex(b"test-payload");
    let h2 = sha256_hex(b"test-payload");
    assert_eq!(h1, h2);
}

#[test]
fn sha256_case_sensitive() {
    // Capitalising one byte changes the hash.
    assert_ne!(sha256_hex(b"body"), sha256_hex(b"Body"));
}

#[test]
fn sha256_output_is_64_hex_chars() {
    assert_eq!(sha256_hex(b"arbitrary input").len(), 64);
}

// ── Integration test stubs (require database) ────────────────────────────────

// #[tokio::test]
// #[ignore = "requires database with payments.callbacks table"]
// async fn nonce_reuse_returns_replay_error() {
//     // Setup: insert a row into payments.callbacks with nonce = "test-nonce-1"
//     // Call check_nonce_fresh(&pool, "test-nonce-1")
//     // Assert: Err(ReplayError::NonceReused("test-nonce-1"))
// }

// #[tokio::test]
// #[ignore = "requires database"]
// async fn fresh_nonce_returns_ok() {
//     // Call check_nonce_fresh(&pool, "never-seen-nonce-xyz")
//     // Assert: Ok(())
// }

// #[tokio::test]
// #[ignore = "requires database + gateway fixture"]
// async fn verify_callback_rejects_tampered_body() {
//     // Build valid sig for body A; submit body B with that sig
//     // Assert: Err(VerifyError::BadSignature)
// }

// #[tokio::test]
// #[ignore = "requires database + gateway fixture"]
// async fn verify_callback_rejects_stale_timestamp() {
//     // Build valid sig with timestamp = now - 400 seconds
//     // Assert: Err(VerifyError::Replay(ReplayError::TimestampStale(_)))
// }
