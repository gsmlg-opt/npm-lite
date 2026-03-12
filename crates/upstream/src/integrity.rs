//! Tarball integrity verification against upstream-provided hashes.

use bytes::Bytes;
use sha1::Sha1;
use sha2::{Digest, Sha512};
use tracing::{debug, warn};

/// Verify a downloaded tarball's integrity against the packument-provided
/// `shasum` (SHA-1 hex) and/or `integrity` (SRI hash, e.g. `sha512-...`).
///
/// Returns `true` if verification passes (or if no hashes are available to
/// verify against). Returns `false` if a hash mismatch is detected.
pub fn verify_tarball_integrity(
    data: &Bytes,
    expected_shasum: Option<&str>,
    expected_integrity: Option<&str>,
) -> bool {
    // Verify shasum (SHA-1) if provided.
    if let Some(expected) = expected_shasum {
        let mut hasher = Sha1::new();
        hasher.update(data);
        let actual = hex::encode(hasher.finalize());
        if actual != expected {
            warn!(
                expected_shasum = %expected,
                actual_shasum = %actual,
                "tarball shasum mismatch"
            );
            return false;
        }
        debug!(shasum = %actual, "tarball shasum verified");
    }

    // Verify integrity (SRI) if provided.
    if let Some(expected) = expected_integrity
        && let Some(expected_b64) = expected.strip_prefix("sha512-")
    {
        let mut hasher = Sha512::new();
        hasher.update(data);
        let actual_bytes = hasher.finalize();
        use base64::Engine;
        let actual_b64 = base64::engine::general_purpose::STANDARD.encode(actual_bytes);
        if actual_b64 != expected_b64 {
            warn!(
                expected_integrity = %expected,
                "tarball integrity mismatch (sha512)"
            );
            return false;
        }
        debug!("tarball integrity verified (sha512)");
    }
    // If integrity uses an algorithm we don't support, skip verification
    // rather than failing.

    true
}

/// Extract shasum and integrity from a packument for a specific version.
pub fn extract_version_hashes(
    packument: &serde_json::Value,
    version: &str,
) -> (Option<String>, Option<String>) {
    let dist = packument
        .get("versions")
        .and_then(|v| v.get(version))
        .and_then(|v| v.get("dist"));

    let shasum = dist
        .and_then(|d| d.get("shasum"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    let integrity = dist
        .and_then(|d| d.get("integrity"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    (shasum, integrity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn verify_correct_shasum() {
        let data = Bytes::from_static(b"hello world");
        // SHA-1 of "hello world"
        let shasum = "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed";
        assert!(verify_tarball_integrity(&data, Some(shasum), None));
    }

    #[test]
    fn verify_wrong_shasum() {
        let data = Bytes::from_static(b"hello world");
        assert!(!verify_tarball_integrity(
            &data,
            Some("0000000000000000000000000000000000000000"),
            None
        ));
    }

    #[test]
    fn verify_no_hashes_passes() {
        let data = Bytes::from_static(b"anything");
        assert!(verify_tarball_integrity(&data, None, None));
    }

    #[test]
    fn extract_hashes_from_packument() {
        let packument = json!({
            "versions": {
                "1.0.0": {
                    "dist": {
                        "shasum": "abc123",
                        "integrity": "sha512-xyz"
                    }
                }
            }
        });
        let (shasum, integrity) = extract_version_hashes(&packument, "1.0.0");
        assert_eq!(shasum.as_deref(), Some("abc123"));
        assert_eq!(integrity.as_deref(), Some("sha512-xyz"));
    }

    #[test]
    fn extract_hashes_missing_version() {
        let packument = json!({"versions": {}});
        let (shasum, integrity) = extract_version_hashes(&packument, "1.0.0");
        assert!(shasum.is_none());
        assert!(integrity.is_none());
    }
}
