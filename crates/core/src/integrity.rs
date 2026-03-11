use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
// sha1::Digest and sha2::Digest are the same underlying trait (digest::Digest).
// Importing via sha2 is sufficient to call .update() / .finalize() on all hashers.

/// Holds SRI-style integrity hashes for a blob of data.
#[derive(Debug, Clone, PartialEq)]
pub struct IntegrityHashes {
    /// Hex-encoded SHA-1 digest (legacy npm `shasum` field).
    pub shasum: String,

    /// Raw SHA-512 digest bytes.
    pub sha512: Vec<u8>,

    /// SRI integrity string: `sha512-<base64>`.
    pub integrity: String,

    /// Hex-encoded SHA-256 digest.
    pub sha256: String,
}

/// Compute [`IntegrityHashes`] for the given byte slice.
pub fn compute_integrity(data: &[u8]) -> IntegrityHashes {
    // SHA-1 (shasum)
    let mut sha1_hasher = Sha1::new();
    sha1_hasher.update(data);
    let sha1_result = sha1_hasher.finalize();
    let shasum = hex::encode_bytes(&sha1_result);

    // SHA-256
    let mut sha256_hasher = Sha256::new();
    sha256_hasher.update(data);
    let sha256_result = sha256_hasher.finalize();
    let sha256 = hex::encode_bytes(&sha256_result);

    // SHA-512
    let mut sha512_hasher = Sha512::new();
    sha512_hasher.update(data);
    let sha512_result = sha512_hasher.finalize();
    let sha512 = sha512_result.to_vec();
    let sha512_b64 = STANDARD.encode(&sha512);
    let integrity = format!("sha512-{}", sha512_b64);

    IntegrityHashes {
        shasum,
        sha512,
        integrity,
        sha256,
    }
}

/// Hex encoding helpers (avoids pulling in the `hex` crate).
mod hex {
    pub fn encode_bytes(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shasum_is_40_hex_chars() {
        let h = compute_integrity(b"hello world");
        assert_eq!(h.shasum.len(), 40);
        assert!(h.shasum.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sha256_is_64_hex_chars() {
        let h = compute_integrity(b"hello world");
        assert_eq!(h.sha256.len(), 64);
        assert!(h.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sha512_length() {
        let h = compute_integrity(b"hello world");
        assert_eq!(h.sha512.len(), 64); // 512 bits = 64 bytes
    }

    #[test]
    fn integrity_starts_with_prefix() {
        let h = compute_integrity(b"hello world");
        assert!(h.integrity.starts_with("sha512-"));
    }

    #[test]
    fn integrity_base64_decodes_to_sha512() {
        let h = compute_integrity(b"hello world");
        let b64_part = h.integrity.strip_prefix("sha512-").unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64_part)
            .expect("valid base64");
        assert_eq!(decoded, h.sha512);
    }

    #[test]
    fn known_sha1_of_empty() {
        // SHA-1("") = da39a3ee5e6b4b0d3255bfef95601890afd80709
        let h = compute_integrity(b"");
        assert_eq!(h.shasum, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn deterministic_output() {
        let a = compute_integrity(b"npm-lite rocks");
        let b = compute_integrity(b"npm-lite rocks");
        assert_eq!(a, b);
    }

    #[test]
    fn different_inputs_differ() {
        let a = compute_integrity(b"foo");
        let b = compute_integrity(b"bar");
        assert_ne!(a.shasum, b.shasum);
        assert_ne!(a.integrity, b.integrity);
    }
}
