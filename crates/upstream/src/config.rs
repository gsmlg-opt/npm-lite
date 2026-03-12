//! Upstream configuration parsing from environment variables.

use std::env;
use std::time::Duration;

/// Configuration for the upstream proxy feature.
#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    /// The global upstream registry URL (e.g. `https://registry.npmjs.org`).
    /// If `None`, upstream proxying is disabled (fully local mode).
    pub upstream_url: Option<String>,

    /// HTTP timeout for upstream requests.
    pub timeout: Duration,
}

impl UpstreamConfig {
    /// Load upstream configuration from environment variables.
    pub fn from_env() -> Self {
        let upstream_url = env::var("UPSTREAM_URL")
            .ok()
            .filter(|s| !s.is_empty());

        let timeout_secs: u64 = env::var("UPSTREAM_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        Self {
            upstream_url,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// Returns `true` if an upstream URL is configured.
    pub fn is_enabled(&self) -> bool {
        self.upstream_url.is_some()
    }
}
