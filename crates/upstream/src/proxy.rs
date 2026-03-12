//! Packument rewriting and tarball proxy logic.

use serde_json::Value;
use tracing::debug;

/// Rewrite tarball URLs in an upstream packument so they point through this
/// registry instead of directly to the upstream.
///
/// The npm client will then fetch tarballs from *this* registry, which will
/// proxy them from the upstream transparently.
pub fn rewrite_tarball_urls(packument: &mut Value, registry_url: &str) {
    let base = registry_url.trim_end_matches('/');

    let versions = match packument.get_mut("versions") {
        Some(Value::Object(v)) => v,
        _ => return,
    };

    for (_version_str, version_meta) in versions.iter_mut() {
        if let Some(dist) = version_meta.get_mut("dist")
            && let Some(tarball) = dist.get("tarball").and_then(|t| t.as_str())
            && let Some(rewritten) = rewrite_single_url(tarball, base)
        {
            debug!(
                original = %tarball,
                rewritten = %rewritten,
                "rewrote tarball URL"
            );
            dist.as_object_mut()
                .unwrap()
                .insert("tarball".to_string(), Value::String(rewritten));
        }
    }
}

/// Rewrite a single upstream tarball URL to point to this registry.
///
/// Input:  `https://registry.npmjs.org/express/-/express-4.18.2.tgz`
/// Output: `https://this-registry.example.com/express/-/express-4.18.2.tgz`
///
/// Input:  `https://registry.npmjs.org/@babel/core/-/core-7.21.0.tgz`
/// Output: `https://this-registry.example.com/@babel/core/-/core-7.21.0.tgz`
fn rewrite_single_url(tarball_url: &str, registry_base: &str) -> Option<String> {
    // Parse the upstream URL to extract the path portion after the host.
    let url = url::Url::parse(tarball_url).ok()?;
    let path = url.path(); // e.g. "/express/-/express-4.18.2.tgz"

    Some(format!("{}{}", registry_base, path))
}

/// Given an upstream packument and a package name, extract the original tarball
/// URL for a specific version from the upstream packument.
///
/// This is used when the client requests a tarball — we look up the original
/// upstream URL in the packument so we know where to stream from.
pub fn extract_upstream_tarball_url(packument: &Value, version: &str) -> Option<String> {
    packument
        .get("versions")?
        .get(version)?
        .get("dist")?
        .get("tarball")?
        .as_str()
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rewrites_plain_package_tarball_url() {
        let mut packument = json!({
            "name": "express",
            "versions": {
                "4.18.2": {
                    "dist": {
                        "tarball": "https://registry.npmjs.org/express/-/express-4.18.2.tgz",
                        "shasum": "abc123"
                    }
                }
            }
        });

        rewrite_tarball_urls(&mut packument, "https://my-registry.example.com");

        let tarball = packument["versions"]["4.18.2"]["dist"]["tarball"]
            .as_str()
            .unwrap();
        assert_eq!(
            tarball,
            "https://my-registry.example.com/express/-/express-4.18.2.tgz"
        );
    }

    #[test]
    fn rewrites_scoped_package_tarball_url() {
        let mut packument = json!({
            "name": "@babel/core",
            "versions": {
                "7.21.0": {
                    "dist": {
                        "tarball": "https://registry.npmjs.org/@babel/core/-/core-7.21.0.tgz",
                        "shasum": "def456"
                    }
                }
            }
        });

        rewrite_tarball_urls(&mut packument, "https://my-registry.example.com");

        let tarball = packument["versions"]["7.21.0"]["dist"]["tarball"]
            .as_str()
            .unwrap();
        assert_eq!(
            tarball,
            "https://my-registry.example.com/@babel/core/-/core-7.21.0.tgz"
        );
    }

    #[test]
    fn extract_tarball_url_works() {
        let packument = json!({
            "versions": {
                "1.0.0": {
                    "dist": {
                        "tarball": "https://upstream.example.com/pkg/-/pkg-1.0.0.tgz"
                    }
                }
            }
        });

        assert_eq!(
            extract_upstream_tarball_url(&packument, "1.0.0"),
            Some("https://upstream.example.com/pkg/-/pkg-1.0.0.tgz".to_string())
        );
        assert_eq!(extract_upstream_tarball_url(&packument, "2.0.0"), None);
    }

    #[test]
    fn handles_missing_versions() {
        let mut packument = json!({ "name": "empty" });
        rewrite_tarball_urls(&mut packument, "https://example.com");
        // Should not panic, just no-op
        assert_eq!(packument["name"], "empty");
    }

    #[test]
    fn rewrites_multiple_versions() {
        let mut packument = json!({
            "name": "lodash",
            "versions": {
                "4.17.20": {
                    "dist": {
                        "tarball": "https://registry.npmjs.org/lodash/-/lodash-4.17.20.tgz"
                    }
                },
                "4.17.21": {
                    "dist": {
                        "tarball": "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz"
                    }
                }
            }
        });

        rewrite_tarball_urls(&mut packument, "https://my-reg.com");

        assert_eq!(
            packument["versions"]["4.17.20"]["dist"]["tarball"]
                .as_str()
                .unwrap(),
            "https://my-reg.com/lodash/-/lodash-4.17.20.tgz"
        );
        assert_eq!(
            packument["versions"]["4.17.21"]["dist"]["tarball"]
                .as_str()
                .unwrap(),
            "https://my-reg.com/lodash/-/lodash-4.17.21.tgz"
        );
    }

    #[test]
    fn registry_url_trailing_slash_stripped() {
        let mut packument = json!({
            "name": "pkg",
            "versions": {
                "1.0.0": {
                    "dist": {
                        "tarball": "https://registry.npmjs.org/pkg/-/pkg-1.0.0.tgz"
                    }
                }
            }
        });

        rewrite_tarball_urls(&mut packument, "https://my-reg.com/");

        let tarball = packument["versions"]["1.0.0"]["dist"]["tarball"]
            .as_str()
            .unwrap();
        assert_eq!(tarball, "https://my-reg.com/pkg/-/pkg-1.0.0.tgz");
    }

    #[test]
    fn skips_version_without_dist() {
        let mut packument = json!({
            "name": "pkg",
            "versions": {
                "1.0.0": {
                    "name": "pkg"
                }
            }
        });

        rewrite_tarball_urls(&mut packument, "https://my-reg.com");
        // Should not panic; version without dist is left unchanged
        assert!(packument["versions"]["1.0.0"]["dist"].is_null());
    }

    #[test]
    fn extract_returns_none_for_missing_dist() {
        let packument = json!({
            "versions": {
                "1.0.0": {
                    "name": "pkg"
                }
            }
        });
        assert_eq!(extract_upstream_tarball_url(&packument, "1.0.0"), None);
    }
}
