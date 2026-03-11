use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("argon2 error: {0}")]
    Argon2(String),

    #[error("invalid password hash format")]
    InvalidHash,
}

// ---------------------------------------------------------------------------
// Token hashing (SHA-256, hex-encoded)
// ---------------------------------------------------------------------------

/// Hash a raw token using SHA-256 and return the hex-encoded digest.
///
/// This is fast enough for token lookup (stored in the DB, compared on every
/// authenticated request) and doesn't need salting because tokens are already
/// high-entropy random strings.
pub fn hash_token(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Return `true` when `raw` hashes to `hash`.
pub fn verify_token(raw: &str, hash: &str) -> bool {
    hash_token(raw) == hash
}

// ---------------------------------------------------------------------------
// Password hashing (Argon2id)
// ---------------------------------------------------------------------------

/// Hash `password` with Argon2id and return the PHC-formatted string.
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AuthError::Argon2(e.to_string()))
}

/// Verify `password` against a PHC-formatted `hash` produced by [`hash_password`].
pub fn verify_password(password: &str, hash: &str) -> Result<bool, AuthError> {
    let parsed = PasswordHash::new(hash).map_err(|_| AuthError::InvalidHash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

// ---------------------------------------------------------------------------
// Token generation
// ---------------------------------------------------------------------------

/// Generate a cryptographically random, URL-safe token (32 random bytes →
/// 43-character base64url string without padding).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- hash_token / verify_token ---

    #[test]
    fn hash_token_is_64_hex_chars() {
        let h = hash_token("my-secret-token");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn verify_token_correct() {
        let raw = "npm_test_token_abc123";
        let hash = hash_token(raw);
        assert!(verify_token(raw, &hash));
    }

    #[test]
    fn verify_token_wrong_raw() {
        let hash = hash_token("correct-token");
        assert!(!verify_token("wrong-token", &hash));
    }

    #[test]
    fn verify_token_wrong_hash() {
        assert!(!verify_token("token", "0000000000000000000000000000000000000000000000000000000000000000"));
    }

    #[test]
    fn hash_token_is_deterministic() {
        let raw = "deterministic";
        assert_eq!(hash_token(raw), hash_token(raw));
    }

    // --- hash_password / verify_password ---

    #[test]
    fn password_roundtrip() {
        let hash = hash_password("correct-horse-battery-staple").unwrap();
        assert!(verify_password("correct-horse-battery-staple", &hash).unwrap());
    }

    #[test]
    fn wrong_password_fails() {
        let hash = hash_password("correct-password").unwrap();
        assert!(!verify_password("wrong-password", &hash).unwrap());
    }

    #[test]
    fn password_hashes_differ_for_same_input() {
        // Argon2 uses a random salt, so two hashes of the same password must differ.
        let h1 = hash_password("same-password").unwrap();
        let h2 = hash_password("same-password").unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn invalid_hash_returns_error() {
        let result = verify_password("password", "not-a-valid-phc-hash");
        assert!(matches!(result, Err(AuthError::InvalidHash)));
    }

    // --- generate_token ---

    #[test]
    fn generated_token_is_non_empty() {
        let t = generate_token();
        assert!(!t.is_empty());
    }

    #[test]
    fn generated_tokens_are_unique() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2);
    }

    #[test]
    fn generated_token_url_safe() {
        let t = generate_token();
        assert!(t.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn generated_token_length() {
        // 32 bytes → ceil(32 * 4 / 3) = 43 chars (no padding)
        let t = generate_token();
        assert_eq!(t.len(), 43);
    }
}
