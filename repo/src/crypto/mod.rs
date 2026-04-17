/// Application-layer field encryption and data masking.
///
/// ## Encryption scheme
///
/// AES-256-GCM is used for all field-level encryption.  Each ciphertext blob
/// is self-contained:
///
/// ```text
/// | nonce (12 bytes) | ciphertext + GCM tag (n + 16 bytes) |
/// ```
///
/// Blobs are stored as `BYTEA` in PostgreSQL.  The encryption key is supplied
/// via the `ENCRYPTION_KEY` environment variable (64 hex characters = 32 bytes
/// = 256-bit key).
///
/// ## Key rotation
///
/// Set `ENCRYPTION_KEY_PREVIOUS` to the old key while `ENCRYPTION_KEY` holds
/// the new key.  Reads try the current key first; on failure they fall back to
/// the previous key.  Writes always use the current key.  To complete rotation,
/// re-encrypt affected rows (run the re-encrypt background job or migration)
/// then clear `ENCRYPTION_KEY_PREVIOUS`.
///
/// ## Masking rules
///
/// Masking is applied at the API serialisation layer — encrypted plaintext is
/// decrypted server-side only when strictly required for business logic, and is
/// never sent to the client in unmasked form:
///
/// | Field           | Stored as            | API response       |
/// |-----------------|----------------------|--------------------|
/// | card last-4     | encrypted BYTEA      | `****1234`         |
/// | payer_ref       | encrypted BYTEA      | omitted (`has_payer_ref: bool`) |
/// | email           | encrypted BYTEA      | `j***@example.com` |
/// | statement file  | encrypted BYTEA      | never exposed      |
/// | IP address      | INET                 | `192.168.1.xxx`    |
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};

// ============================================================
// Error type
// ============================================================

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("invalid key — must be exactly 64 hex characters (32 bytes): {0}")]
    InvalidKey(String),
    #[error("encryption failed")]
    EncryptFailed,
    #[error("decryption failed — wrong key or corrupted ciphertext")]
    DecryptFailed,
}

// ============================================================
// Field encryptor
// ============================================================

/// AES-256-GCM field encryptor.
///
/// Cheaply cloneable; should be stored in `AppState` and shared across
/// request handlers.  Both `Aes256Gcm` variants are `Send + Sync`.
#[derive(Clone)]
pub struct FieldEncryptor {
    current:  Aes256Gcm,
    previous: Option<Aes256Gcm>,
}

impl FieldEncryptor {
    /// Build from a 64-hex-character string (32 bytes / AES-256).
    ///
    /// Panics on invalid input — call at startup so misconfiguration is caught
    /// immediately rather than at first encryption attempt.
    pub fn from_hex_key(hex: &str) -> Result<Self, CryptoError> {
        let cipher = cipher_from_hex(hex)?;
        Ok(Self { current: cipher, previous: None })
    }

    /// Register a previous key to support in-flight key rotation.
    ///
    /// `decrypt` will fall back to the previous key if the current key fails.
    /// `encrypt` always uses the current key.
    pub fn with_previous_key(mut self, prev_hex: &str) -> Result<Self, CryptoError> {
        self.previous = Some(cipher_from_hex(prev_hex)?);
        Ok(self)
    }

    // ---- Core byte-level operations ----

    /// Encrypt arbitrary bytes.  Returns `nonce[12] || ciphertext+tag[n+16]`.
    pub fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self.current
            .encrypt(&nonce, plaintext)
            .map_err(|_| CryptoError::EncryptFailed)?;

        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypt bytes produced by [`encrypt_bytes`].
    ///
    /// Tries the current key first; falls back to the previous key so that
    /// data encrypted before a key rotation can still be read during rollover.
    pub fn decrypt_bytes(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if data.len() < 12 {
            return Err(CryptoError::DecryptFailed);
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        if let Ok(plain) = self.current.decrypt(nonce, ciphertext) {
            return Ok(plain);
        }

        // Fall back to previous key (key-rotation window)
        if let Some(ref prev) = self.previous {
            if let Ok(plain) = prev.decrypt(nonce, ciphertext) {
                return Ok(plain);
            }
        }

        Err(CryptoError::DecryptFailed)
    }

    // ---- String convenience wrappers ----

    /// Encrypt a UTF-8 string field.
    pub fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, CryptoError> {
        self.encrypt_bytes(plaintext.as_bytes())
    }

    /// Decrypt a UTF-8 string field.
    pub fn decrypt(&self, data: &[u8]) -> Result<String, CryptoError> {
        let bytes = self.decrypt_bytes(data)?;
        String::from_utf8(bytes).map_err(|_| CryptoError::DecryptFailed)
    }

    /// Encrypt an optional string field — returns `None` when input is `None`.
    pub fn encrypt_opt(&self, val: Option<&str>) -> Result<Option<Vec<u8>>, CryptoError> {
        val.map(|v| self.encrypt(v)).transpose()
    }

    /// Decrypt an optional blob — returns `None` when input is `None`.
    pub fn decrypt_opt(&self, val: Option<&[u8]>) -> Result<Option<String>, CryptoError> {
        val.map(|b| self.decrypt(b)).transpose()
    }
}

// ---- Private helpers ----

fn cipher_from_hex(hex: &str) -> Result<Aes256Gcm, CryptoError> {
    if hex.len() != 64 {
        return Err(CryptoError::InvalidKey(format!(
            "got {} characters, need 64",
            hex.len()
        )));
    }
    let key_bytes: Vec<u8> = ::hex::decode(hex).map_err(|_| {
        CryptoError::InvalidKey("non-hex characters present".to_string())
    })?;
    // key_bytes is always 32 bytes at this point; from_slice panics only on wrong length
    Ok(Aes256Gcm::new_from_slice(&key_bytes).expect("key length validated above"))
}

// ============================================================
// Masking functions  (no key required — pure string transform)
// ============================================================

/// Mask a card last-4 value for display: `"1234"` → `"****1234"`.
///
/// Combines with the scheme of never storing the full PAN — only the last-4
/// suffix is kept, then masked with bullet prefix in the response.
pub fn mask_card_last4(last4: &str) -> String {
    format!("****{}", last4)
}

/// Mask an email address: `"john.doe@example.com"` → `"j***@example.com"`.
pub fn mask_email(email: &str) -> String {
    match email.rfind('@') {
        Some(at) => {
            let local  = &email[..at];
            let domain = &email[at..];
            let first  = local.chars().next().map(|c| c.to_string()).unwrap_or_default();
            format!("{}***{}", first, domain)
        }
        None => "***".to_string(),
    }
}

/// Mask an account number / IBAN: show first 4 and last 4 characters.
///
/// `"GB29NWBK60161331926819"` → `"GB29***6819"`
pub fn mask_account_number(number: &str) -> String {
    let n = number.len();
    if n <= 8 {
        return "*".repeat(n);
    }
    format!("{}***{}", &number[..4], &number[n - 4..])
}

/// Mask an IPv4 address: replace the host octet with `xxx`.
///
/// `"192.168.1.42"` → `"192.168.1.xxx"`
/// IPv6 / unusual formats: replace everything after the last `:` or return `xxx`.
pub fn mask_ip(ip: &str) -> String {
    // IPv4
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() == 4 {
        return format!("{}.{}.{}.xxx", parts[0], parts[1], parts[2]);
    }
    // IPv6
    let segments: Vec<&str> = ip.split(':').collect();
    if segments.len() >= 3 {
        // Show first two segments, mask the rest
        return format!("{}:{}:xxxx", segments[0], segments[1]);
    }
    "xxx".to_string()
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn test_encryptor() -> FieldEncryptor {
        // 32 zero bytes as test key (64 hex chars)
        FieldEncryptor::from_hex_key(&"00".repeat(32)).unwrap()
    }

    #[test]
    fn round_trip_string() {
        let enc = test_encryptor();
        let ct  = enc.encrypt("hello world").unwrap();
        assert_eq!(enc.decrypt(&ct).unwrap(), "hello world");
    }

    #[test]
    fn round_trip_bytes() {
        let enc  = test_encryptor();
        let data = b"\x00\x01\x02\xFF";
        let ct   = enc.encrypt_bytes(data).unwrap();
        assert_eq!(enc.decrypt_bytes(&ct).unwrap(), data);
    }

    #[test]
    fn different_nonces_each_call() {
        let enc = test_encryptor();
        let ct1 = enc.encrypt("same plaintext").unwrap();
        let ct2 = enc.encrypt("same plaintext").unwrap();
        // probabilistically different nonces → different ciphertexts
        assert_ne!(ct1, ct2);
    }

    #[test]
    fn wrong_key_fails() {
        let enc1 = test_encryptor();
        let enc2 = FieldEncryptor::from_hex_key(&"AA".repeat(32)).unwrap();
        let ct = enc1.encrypt("secret").unwrap();
        assert!(enc2.decrypt(&ct).is_err());
    }

    #[test]
    fn previous_key_fallback() {
        let old_key = "00".repeat(32);
        let new_key = "AA".repeat(32);
        let old_enc = FieldEncryptor::from_hex_key(&old_key).unwrap();
        let new_enc = FieldEncryptor::from_hex_key(&new_key)
            .unwrap()
            .with_previous_key(&old_key)
            .unwrap();
        // Data encrypted with old key can be decrypted by new encryptor
        let ct = old_enc.encrypt("rotate me").unwrap();
        assert_eq!(new_enc.decrypt(&ct).unwrap(), "rotate me");
    }

    #[test]
    fn mask_card_formats() {
        assert_eq!(mask_card_last4("1234"), "****1234");
        assert_eq!(mask_card_last4("9999"), "****9999");
    }

    #[test]
    fn mask_email_formats() {
        assert_eq!(mask_email("john.doe@example.com"), "j***@example.com");
        assert_eq!(mask_email("a@b.com"),              "a***@b.com");
        assert_eq!(mask_email("noatsign"),             "***");
    }

    #[test]
    fn mask_account_number_formats() {
        // Long IBAN → first 4 + "***" + last 4.
        assert_eq!(mask_account_number("GB29NWBK60161331926819"), "GB29***6819");
        // 9-char minimum for the truncated form (the impl treats n ≤ 8 as too short).
        assert_eq!(mask_account_number("123456789"), "1234***6789");
        // n ≤ 8 are fully masked to avoid leaking short account numbers.
        assert_eq!(mask_account_number("12345678"), "********");
        assert_eq!(mask_account_number("short"),    "*****");
    }

    #[test]
    fn mask_ip_formats() {
        assert_eq!(mask_ip("192.168.1.42"),  "192.168.1.xxx");
        assert_eq!(mask_ip("10.0.0.1"),      "10.0.0.xxx");
        assert_eq!(mask_ip("2001:db8:85a3:08d3::1"), "2001:db8:xxxx");
    }
}
