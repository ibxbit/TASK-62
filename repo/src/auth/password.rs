/// Password hashing using **argon2id** via the `argon2` crate.
///
/// Strategy:
/// - Algorithm:  Argon2id (resistant to both side-channel and GPU attacks)
/// - Parameters: crate defaults (m=19MiB, t=2, p=1) — ~100 ms on reference HW
/// - Salt:       16 random bytes generated per hash via `OsRng`
/// - Output:     PHC string format — includes algorithm, params, salt, and hash in one string
///
/// The PHC string is stored directly in `auth.users.password_hash` (TEXT column).
/// No separate salt column is needed; the salt is embedded in the PHC string.
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

/// Hash `password` and return the PHC string suitable for database storage.
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt   = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default(); // argon2id variant
    Ok(argon2.hash_password(password.as_bytes(), &salt)?.to_string())
}

/// Return `true` if `password` matches the stored PHC `hash`, `false` otherwise.
///
/// Timing: argon2 verification is deliberately slow (~100 ms) which naturally
/// rate-limits offline dictionary attacks. The comparison itself is constant-time.
pub fn verify_password(
    password: &str,
    hash: &str,
) -> Result<bool, argon2::password_hash::Error> {
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
