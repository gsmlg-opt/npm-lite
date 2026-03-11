use std::fmt;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Role
// ---------------------------------------------------------------------------

/// Access role for a user or token within the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Read-only access (install / download packages).
    Read,
    /// Can publish / unpublish packages they own.
    Publish,
    /// Full administrative access.
    Admin,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Read => write!(f, "read"),
            Role::Publish => write!(f, "publish"),
            Role::Admin => write!(f, "admin"),
        }
    }
}

// ---------------------------------------------------------------------------
// PackageName
// ---------------------------------------------------------------------------

/// A validated, normalized npm package name.
///
/// Scoped packages carry an optional `scope` (without the leading `@`).
/// The `name` field is the bare package name (without `@scope/` prefix).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PackageName {
    /// Scope without the leading `@`, e.g. `"myorg"` for `@myorg/pkg`.
    pub scope: Option<String>,
    /// Bare package name, e.g. `"pkg"`.
    pub name: String,
}

impl PackageName {
    /// Construct a plain (non-scoped) package name.
    pub fn plain(name: impl Into<String>) -> Self {
        Self {
            scope: None,
            name: name.into(),
        }
    }

    /// Construct a scoped package name.
    pub fn scoped(scope: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            scope: Some(scope.into()),
            name: name.into(),
        }
    }

    /// Parse from a validated name string (output of `validate_package_name`).
    ///
    /// Returns `None` if the string is empty or malformed.
    pub fn parse(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }
        if let Some(rest) = s.strip_prefix('@') {
            let mut parts = rest.splitn(2, '/');
            let scope = parts.next()?.to_string();
            let name = parts.next()?.to_string();
            if scope.is_empty() || name.is_empty() {
                return None;
            }
            Some(Self::scoped(scope, name))
        } else {
            Some(Self::plain(s))
        }
    }

    /// Returns `true` if this is a scoped package.
    pub fn is_scoped(&self) -> bool {
        self.scope.is_some()
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.scope {
            Some(scope) => write!(f, "@{}/{}", scope, self.name),
            None => write!(f, "{}", self.name),
        }
    }
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Top-level error type for the npm-core domain.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("validation error: {0}")]
    Validation(#[from] crate::validation::ValidationError),

    #[error("authentication error: {0}")]
    Auth(#[from] crate::auth::AuthError),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Role ---

    #[test]
    fn role_display() {
        assert_eq!(Role::Read.to_string(), "read");
        assert_eq!(Role::Publish.to_string(), "publish");
        assert_eq!(Role::Admin.to_string(), "admin");
    }

    #[test]
    fn role_serde_roundtrip() {
        let json = serde_json::to_string(&Role::Admin).unwrap();
        assert_eq!(json, "\"admin\"");
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Role::Admin);
    }

    // --- PackageName ---

    #[test]
    fn plain_display() {
        let pkg = PackageName::plain("react");
        assert_eq!(pkg.to_string(), "react");
        assert!(!pkg.is_scoped());
    }

    #[test]
    fn scoped_display() {
        let pkg = PackageName::scoped("myorg", "utils");
        assert_eq!(pkg.to_string(), "@myorg/utils");
        assert!(pkg.is_scoped());
    }

    #[test]
    fn parse_plain() {
        let pkg = PackageName::parse("lodash").unwrap();
        assert_eq!(pkg.scope, None);
        assert_eq!(pkg.name, "lodash");
    }

    #[test]
    fn parse_scoped() {
        let pkg = PackageName::parse("@babel/core").unwrap();
        assert_eq!(pkg.scope.as_deref(), Some("babel"));
        assert_eq!(pkg.name, "core");
    }

    #[test]
    fn parse_empty_returns_none() {
        assert!(PackageName::parse("").is_none());
    }

    #[test]
    fn parse_malformed_scope_returns_none() {
        assert!(PackageName::parse("@/name").is_none());
        assert!(PackageName::parse("@scope/").is_none());
    }

    #[test]
    fn serde_roundtrip_plain() {
        let pkg = PackageName::plain("express");
        let json = serde_json::to_string(&pkg).unwrap();
        let back: PackageName = serde_json::from_str(&json).unwrap();
        assert_eq!(pkg, back);
    }

    #[test]
    fn serde_roundtrip_scoped() {
        let pkg = PackageName::scoped("types", "node");
        let json = serde_json::to_string(&pkg).unwrap();
        let back: PackageName = serde_json::from_str(&json).unwrap();
        assert_eq!(pkg, back);
    }
}
