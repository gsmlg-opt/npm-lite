//! Upstream routing: determines which upstream (if any) to use for a package.
//!
//! Evaluation order per PRD §3.1:
//! 1. Per-scope rules — exact scope match
//! 2. Per-pattern rules — regex match (first match wins)
//! 3. Global upstream — fallback
//! 4. No upstream — return None

use crate::config::UpstreamConfig;
use regex::Regex;
use tracing::{debug, warn};

/// The result of evaluating upstream routing rules for a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteTarget {
    /// Proxy to this upstream URL.
    Upstream(String),
    /// Package must be resolved locally only; never proxy.
    Local,
    /// No routing rule matched and no global upstream is configured.
    None,
}

/// Evaluate upstream routing rules for the given package name.
///
/// Returns a [`RouteTarget`] indicating where to look for the package.
pub fn resolve(config: &UpstreamConfig, package_name: &str) -> RouteTarget {
    // Extract scope from scoped packages (e.g. "@babel/core" → "@babel").
    let scope = extract_scope(package_name);

    // 1. Check local_scopes deny-list (never proxy these scopes).
    if let Some(scope) = &scope
        && config.local_scopes.iter().any(|s| s == scope)
    {
        debug!(package = %package_name, scope = %scope, "scope in local_scopes deny-list");
        return RouteTarget::Local;
    }

    // 2. Check per-scope rules.
    if let Some(scope) = &scope
        && let Some(target) = config.scope_rules.get(scope.as_str())
    {
        if target == "local" {
            debug!(package = %package_name, scope = %scope, "scope rule → local");
            return RouteTarget::Local;
        }
        debug!(package = %package_name, scope = %scope, target = %target, "scope rule matched");
        return RouteTarget::Upstream(target.clone());
    }

    // 3. Check per-pattern rules (evaluated in order, first match wins).
    for rule in &config.pattern_rules {
        match Regex::new(&rule.pattern) {
            Ok(re) => {
                if re.is_match(package_name) {
                    if rule.target == "local" {
                        debug!(package = %package_name, pattern = %rule.pattern, "pattern rule → local");
                        return RouteTarget::Local;
                    }
                    debug!(
                        package = %package_name,
                        pattern = %rule.pattern,
                        target = %rule.target,
                        "pattern rule matched"
                    );
                    return RouteTarget::Upstream(rule.target.clone());
                }
            }
            Err(e) => {
                warn!(pattern = %rule.pattern, error = %e, "invalid upstream pattern regex, skipping");
            }
        }
    }

    // 4. Global upstream fallback.
    if let Some(url) = &config.upstream_url {
        debug!(package = %package_name, upstream = %url, "using global upstream");
        return RouteTarget::Upstream(url.clone());
    }

    // 5. No upstream configured.
    RouteTarget::None
}

/// Extract the scope from a scoped package name.
/// e.g. "@babel/core" → Some("@babel"), "express" → None
fn extract_scope(package_name: &str) -> Option<String> {
    if package_name.starts_with('@')
        && let Some(slash_pos) = package_name.find('/')
    {
        return Some(package_name[..slash_pos].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PatternRule;
    use std::collections::HashMap;
    use std::time::Duration;

    fn base_config() -> UpstreamConfig {
        UpstreamConfig {
            upstream_url: Some("https://registry.npmjs.org".to_string()),
            timeout: Duration::from_secs(30),
            cache_enabled: false,
            cache_ttl: Duration::from_secs(300),
            scope_rules: HashMap::new(),
            local_scopes: Vec::new(),
            pattern_rules: Vec::new(),
            auth_token_refs: HashMap::new(),
        }
    }

    #[test]
    fn unscoped_package_uses_global() {
        let config = base_config();
        assert_eq!(
            resolve(&config, "express"),
            RouteTarget::Upstream("https://registry.npmjs.org".to_string())
        );
    }

    #[test]
    fn scoped_package_uses_global_when_no_scope_rule() {
        let config = base_config();
        assert_eq!(
            resolve(&config, "@babel/core"),
            RouteTarget::Upstream("https://registry.npmjs.org".to_string())
        );
    }

    #[test]
    fn scope_rule_overrides_global() {
        let mut config = base_config();
        config.scope_rules.insert(
            "@partner".to_string(),
            "https://partner-registry.example.com".to_string(),
        );

        assert_eq!(
            resolve(&config, "@partner/sdk"),
            RouteTarget::Upstream("https://partner-registry.example.com".to_string())
        );
        // Other scopes still use global
        assert_eq!(
            resolve(&config, "@babel/core"),
            RouteTarget::Upstream("https://registry.npmjs.org".to_string())
        );
    }

    #[test]
    fn scope_rule_local_blocks_proxy() {
        let mut config = base_config();
        config
            .scope_rules
            .insert("@mycompany".to_string(), "local".to_string());

        assert_eq!(resolve(&config, "@mycompany/utils"), RouteTarget::Local);
    }

    #[test]
    fn local_scopes_deny_list() {
        let mut config = base_config();
        config.local_scopes = vec!["@internal".to_string()];

        assert_eq!(resolve(&config, "@internal/secret"), RouteTarget::Local);
    }

    #[test]
    fn local_scopes_takes_precedence_over_scope_rules() {
        let mut config = base_config();
        config.local_scopes = vec!["@private".to_string()];
        config.scope_rules.insert(
            "@private".to_string(),
            "https://some-upstream.example.com".to_string(),
        );

        assert_eq!(resolve(&config, "@private/pkg"), RouteTarget::Local);
    }

    #[test]
    fn no_upstream_configured() {
        let config = UpstreamConfig {
            upstream_url: None,
            timeout: Duration::from_secs(30),
            cache_enabled: false,
            cache_ttl: Duration::from_secs(300),
            scope_rules: HashMap::new(),
            local_scopes: Vec::new(),
            pattern_rules: Vec::new(),
            auth_token_refs: HashMap::new(),
        };

        assert_eq!(resolve(&config, "express"), RouteTarget::None);
    }

    #[test]
    fn extract_scope_works() {
        assert_eq!(extract_scope("@babel/core"), Some("@babel".to_string()));
        assert_eq!(extract_scope("express"), None);
        assert_eq!(extract_scope("@solo"), None);
    }

    #[test]
    fn pattern_rule_matches() {
        let mut config = base_config();
        config.pattern_rules = vec![PatternRule {
            pattern: "^internal-.*".to_string(),
            target: "local".to_string(),
        }];

        assert_eq!(resolve(&config, "internal-utils"), RouteTarget::Local);
        assert_eq!(
            resolve(&config, "express"),
            RouteTarget::Upstream("https://registry.npmjs.org".to_string())
        );
    }

    #[test]
    fn pattern_rule_routes_to_upstream() {
        let mut config = base_config();
        config.pattern_rules = vec![PatternRule {
            pattern: "^legacy-.*".to_string(),
            target: "https://legacy-registry.example.com".to_string(),
        }];

        assert_eq!(
            resolve(&config, "legacy-auth"),
            RouteTarget::Upstream("https://legacy-registry.example.com".to_string())
        );
    }

    #[test]
    fn pattern_first_match_wins() {
        let mut config = base_config();
        config.pattern_rules = vec![
            PatternRule {
                pattern: "^foo-.*".to_string(),
                target: "https://first.example.com".to_string(),
            },
            PatternRule {
                pattern: "^foo-bar.*".to_string(),
                target: "https://second.example.com".to_string(),
            },
        ];

        // First pattern matches, second is never checked.
        assert_eq!(
            resolve(&config, "foo-bar"),
            RouteTarget::Upstream("https://first.example.com".to_string())
        );
    }

    #[test]
    fn scope_rules_take_precedence_over_patterns() {
        let mut config = base_config();
        config
            .scope_rules
            .insert("@myco".to_string(), "local".to_string());
        config.pattern_rules = vec![PatternRule {
            pattern: ".*".to_string(),
            target: "https://catchall.example.com".to_string(),
        }];

        // Scope rule wins over pattern
        assert_eq!(resolve(&config, "@myco/pkg"), RouteTarget::Local);
        // Unscoped falls through to pattern
        assert_eq!(
            resolve(&config, "some-pkg"),
            RouteTarget::Upstream("https://catchall.example.com".to_string())
        );
    }

    #[test]
    fn invalid_pattern_is_skipped() {
        let mut config = base_config();
        config.pattern_rules = vec![PatternRule {
            pattern: "[invalid".to_string(),
            target: "local".to_string(),
        }];

        // Invalid regex is skipped, falls through to global
        assert_eq!(
            resolve(&config, "some-pkg"),
            RouteTarget::Upstream("https://registry.npmjs.org".to_string())
        );
    }
}
