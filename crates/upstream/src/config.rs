//! Upstream configuration parsing from environment variables and TOML files.

use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::time::Duration;
use tracing::{info, warn};

/// Top-level upstream configuration.
#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    /// The global upstream registry URL (e.g. `https://registry.npmjs.org`).
    /// If `None`, upstream proxying is disabled (fully local mode).
    pub upstream_url: Option<String>,

    /// HTTP timeout for upstream requests.
    pub timeout: Duration,

    /// Whether caching of upstream metadata and tarballs is enabled.
    pub cache_enabled: bool,

    /// How long cached metadata is considered fresh (seconds).
    pub cache_ttl: Duration,

    /// Per-scope routing rules: scope name → upstream URL (or "local").
    pub scope_rules: HashMap<String, String>,

    /// Scopes that should never be proxied (always local-only).
    pub local_scopes: Vec<String>,

    /// Pattern-based routing rules (regex → upstream URL). Evaluated in order.
    pub pattern_rules: Vec<PatternRule>,
}

/// A regex-based routing rule.
#[derive(Debug, Clone)]
pub struct PatternRule {
    pub pattern: String,
    pub target: String,
}

/// TOML file structure for upstream configuration.
#[derive(Debug, Deserialize, Default)]
struct TomlConfig {
    upstream: Option<TomlUpstream>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlUpstream {
    url: Option<String>,
    cache_enabled: Option<bool>,
    cache_ttl_secs: Option<u64>,
    timeout_secs: Option<u64>,
    local_scopes: Option<TomlLocalScopes>,
    scopes: Option<HashMap<String, String>>,
    patterns: Option<Vec<TomlPatternRule>>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlLocalScopes {
    scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TomlPatternRule {
    pattern: String,
    target: String,
}

impl UpstreamConfig {
    /// Load upstream configuration from environment variables, optionally
    /// overlaying settings from a TOML configuration file.
    pub fn from_env() -> Self {
        let upstream_url = env::var("UPSTREAM_URL")
            .ok()
            .filter(|s| !s.is_empty());

        let timeout_secs: u64 = env::var("UPSTREAM_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let cache_enabled: bool = env::var("UPSTREAM_CACHE_ENABLED")
            .ok()
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let cache_ttl_secs: u64 = env::var("UPSTREAM_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);

        let mut config = Self {
            upstream_url,
            timeout: Duration::from_secs(timeout_secs),
            cache_enabled,
            cache_ttl: Duration::from_secs(cache_ttl_secs),
            scope_rules: HashMap::new(),
            local_scopes: Vec::new(),
            pattern_rules: Vec::new(),
        };

        // Overlay TOML config if path is specified.
        if let Ok(path) = env::var("UPSTREAM_CONFIG_PATH")
            && !path.is_empty() {
                match std::fs::read_to_string(&path) {
                    Ok(contents) => match toml::from_str::<TomlConfig>(&contents) {
                        Ok(toml_cfg) => {
                            config.apply_toml(toml_cfg);
                            info!(path = %path, "loaded upstream TOML config");
                        }
                        Err(e) => {
                            warn!(path = %path, error = %e, "failed to parse upstream TOML config");
                        }
                    },
                    Err(e) => {
                        warn!(path = %path, error = %e, "failed to read upstream TOML config");
                    }
                }
            }

        config
    }

    /// Apply settings from a parsed TOML configuration.
    fn apply_toml(&mut self, toml: TomlConfig) {
        if let Some(upstream) = toml.upstream {
            // TOML url overrides env var only if env var is not set.
            if self.upstream_url.is_none() {
                self.upstream_url = upstream.url;
            }
            if let Some(enabled) = upstream.cache_enabled {
                self.cache_enabled = enabled;
            }
            if let Some(ttl) = upstream.cache_ttl_secs {
                self.cache_ttl = Duration::from_secs(ttl);
            }
            if let Some(timeout) = upstream.timeout_secs {
                self.timeout = Duration::from_secs(timeout);
            }
            if let Some(local) = upstream.local_scopes {
                self.local_scopes = local.scopes;
            }
            if let Some(scopes) = upstream.scopes {
                self.scope_rules = scopes;
            }
            if let Some(patterns) = upstream.patterns {
                self.pattern_rules = patterns
                    .into_iter()
                    .map(|p| PatternRule {
                        pattern: p.pattern,
                        target: p.target,
                    })
                    .collect();
            }
        }
    }

    /// Returns `true` if an upstream URL is configured.
    pub fn is_enabled(&self) -> bool {
        self.upstream_url.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_when_no_url() {
        let config = UpstreamConfig {
            upstream_url: None,
            timeout: Duration::from_secs(30),
            cache_enabled: false,
            cache_ttl: Duration::from_secs(300),
            scope_rules: HashMap::new(),
            local_scopes: Vec::new(),
            pattern_rules: Vec::new(),
        };
        assert!(!config.is_enabled());
    }

    #[test]
    fn enabled_when_url_set() {
        let config = UpstreamConfig {
            upstream_url: Some("https://registry.npmjs.org".to_string()),
            timeout: Duration::from_secs(30),
            cache_enabled: false,
            cache_ttl: Duration::from_secs(300),
            scope_rules: HashMap::new(),
            local_scopes: Vec::new(),
            pattern_rules: Vec::new(),
        };
        assert!(config.is_enabled());
    }

    #[test]
    fn default_timeout_is_30s() {
        let config = UpstreamConfig {
            upstream_url: None,
            timeout: Duration::from_secs(30),
            cache_enabled: false,
            cache_ttl: Duration::from_secs(300),
            scope_rules: HashMap::new(),
            local_scopes: Vec::new(),
            pattern_rules: Vec::new(),
        };
        assert_eq!(config.timeout, Duration::from_secs(30));
    }

    #[test]
    fn apply_toml_sets_scope_rules() {
        let mut config = UpstreamConfig {
            upstream_url: None,
            timeout: Duration::from_secs(30),
            cache_enabled: false,
            cache_ttl: Duration::from_secs(300),
            scope_rules: HashMap::new(),
            local_scopes: Vec::new(),
            pattern_rules: Vec::new(),
        };

        let toml_str = r#"
            [upstream]
            url = "https://registry.npmjs.org"
            cache_enabled = true
            cache_ttl_secs = 600

            [upstream.local_scopes]
            scopes = ["@mycompany", "@internal"]

            [upstream.scopes]
            "@partner" = "https://partner-registry.example.com"
        "#;

        let toml_cfg: TomlConfig = toml::from_str(toml_str).unwrap();
        config.apply_toml(toml_cfg);

        assert_eq!(
            config.upstream_url.as_deref(),
            Some("https://registry.npmjs.org")
        );
        assert!(config.cache_enabled);
        assert_eq!(config.cache_ttl, Duration::from_secs(600));
        assert_eq!(config.local_scopes, vec!["@mycompany", "@internal"]);
        assert_eq!(
            config.scope_rules.get("@partner").map(|s| s.as_str()),
            Some("https://partner-registry.example.com")
        );
    }

    #[test]
    fn env_url_takes_precedence_over_toml() {
        let mut config = UpstreamConfig {
            upstream_url: Some("https://env-registry.example.com".to_string()),
            timeout: Duration::from_secs(30),
            cache_enabled: false,
            cache_ttl: Duration::from_secs(300),
            scope_rules: HashMap::new(),
            local_scopes: Vec::new(),
            pattern_rules: Vec::new(),
        };

        let toml_str = r#"
            [upstream]
            url = "https://toml-registry.example.com"
        "#;

        let toml_cfg: TomlConfig = toml::from_str(toml_str).unwrap();
        config.apply_toml(toml_cfg);

        // Env var URL should NOT be overwritten by TOML
        assert_eq!(
            config.upstream_url.as_deref(),
            Some("https://env-registry.example.com")
        );
    }

    #[test]
    fn apply_toml_sets_pattern_rules() {
        let mut config = UpstreamConfig {
            upstream_url: None,
            timeout: Duration::from_secs(30),
            cache_enabled: false,
            cache_ttl: Duration::from_secs(300),
            scope_rules: HashMap::new(),
            local_scopes: Vec::new(),
            pattern_rules: Vec::new(),
        };

        let toml_str = r#"
            [upstream]
            url = "https://registry.npmjs.org"

            [[upstream.patterns]]
            pattern = "^internal-.*"
            target = "local"

            [[upstream.patterns]]
            pattern = "^legacy-.*"
            target = "https://legacy-registry.example.com"
        "#;

        let toml_cfg: TomlConfig = toml::from_str(toml_str).unwrap();
        config.apply_toml(toml_cfg);

        assert_eq!(config.pattern_rules.len(), 2);
        assert_eq!(config.pattern_rules[0].pattern, "^internal-.*");
        assert_eq!(config.pattern_rules[0].target, "local");
        assert_eq!(config.pattern_rules[1].pattern, "^legacy-.*");
        assert_eq!(
            config.pattern_rules[1].target,
            "https://legacy-registry.example.com"
        );
    }
}
