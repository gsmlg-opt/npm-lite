use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ValidationError {
    #[error("package name is empty")]
    Empty,

    #[error("package name exceeds 214 characters")]
    TooLong,

    #[error("package name must not start with a dot or underscore")]
    InvalidStart,

    #[error("package name contains invalid characters: {0}")]
    InvalidCharacters(String),

    #[error("scoped package name is malformed: {0}")]
    MalformedScope(String),

    #[error("package name contains uppercase letters; use lowercase")]
    UppercaseLetters,
}

/// Validate and normalize a package name following npm registry rules.
///
/// Rules enforced:
/// - Must not be empty
/// - Must not exceed 214 characters
/// - Must not start with `.` or `_`
/// - Must not contain spaces
/// - For plain names: only `[a-z0-9\-\.]` are allowed
/// - For scoped names `@scope/name`: scope and name each follow the same rules
/// - Returns the lowercased (normalized) name
pub fn validate_package_name(name: &str) -> Result<String, ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::Empty);
    }

    if name.len() > 214 {
        return Err(ValidationError::TooLong);
    }

    // Lowercase for normalization; we still accept uppercase input but normalize it.
    // npm itself rejects uppercase in new packages, so we do too.
    if name.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(ValidationError::UppercaseLetters);
    }

    let lowered = name.to_lowercase();

    if lowered.starts_with('@') {
        validate_scoped(&lowered)
    } else {
        validate_plain(&lowered)?;
        Ok(lowered)
    }
}

/// Validate a plain (non-scoped) package name segment.
fn validate_plain(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::Empty);
    }

    if name.starts_with('.') || name.starts_with('_') {
        return Err(ValidationError::InvalidStart);
    }

    let invalid: String = name.chars().filter(|c| !is_valid_name_char(*c)).collect();

    if !invalid.is_empty() {
        return Err(ValidationError::InvalidCharacters(invalid));
    }

    Ok(())
}

/// Validate a scoped package name of the form `@scope/name`.
fn validate_scoped(name: &str) -> Result<String, ValidationError> {
    // Strip leading `@`
    let rest = &name[1..];

    let slash_pos = rest.find('/').ok_or_else(|| {
        ValidationError::MalformedScope("scoped package must have the form @scope/name".to_string())
    })?;

    let scope = &rest[..slash_pos];
    let pkg = &rest[slash_pos + 1..];

    if scope.is_empty() {
        return Err(ValidationError::MalformedScope(
            "scope part must not be empty".to_string(),
        ));
    }

    if pkg.is_empty() {
        return Err(ValidationError::MalformedScope(
            "name part of scoped package must not be empty".to_string(),
        ));
    }

    validate_plain(scope).map_err(|e| match e {
        ValidationError::InvalidStart => ValidationError::MalformedScope(format!(
            "scope '{}' must not start with a dot or underscore",
            scope
        )),
        ValidationError::InvalidCharacters(chars) => ValidationError::MalformedScope(format!(
            "scope '{}' contains invalid characters: {}",
            scope, chars
        )),
        other => other,
    })?;

    validate_plain(pkg).map_err(|e| match e {
        ValidationError::InvalidStart => ValidationError::MalformedScope(format!(
            "name '{}' must not start with a dot or underscore",
            pkg
        )),
        ValidationError::InvalidCharacters(chars) => ValidationError::MalformedScope(format!(
            "name '{}' contains invalid characters: {}",
            pkg, chars
        )),
        other => other,
    })?;

    Ok(name.to_string())
}

/// Returns `true` for characters allowed in a plain package name segment.
/// Allowed: lowercase ASCII letters, digits, hyphens, and dots.
fn is_valid_name_char(c: char) -> bool {
    matches!(c, 'a'..='z' | '0'..='9' | '-' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- valid names ---

    #[test]
    fn valid_simple_name() {
        assert_eq!(validate_package_name("react"), Ok("react".to_string()));
    }

    #[test]
    fn valid_name_with_hyphen() {
        assert_eq!(
            validate_package_name("my-package"),
            Ok("my-package".to_string())
        );
    }

    #[test]
    fn valid_name_with_dot() {
        assert_eq!(
            validate_package_name("some.pkg"),
            Ok("some.pkg".to_string())
        );
    }

    #[test]
    fn valid_name_with_digits() {
        assert_eq!(validate_package_name("pkg123"), Ok("pkg123".to_string()));
    }

    #[test]
    fn valid_scoped_name() {
        assert_eq!(
            validate_package_name("@scope/package"),
            Ok("@scope/package".to_string())
        );
    }

    #[test]
    fn valid_scoped_name_with_hyphen() {
        assert_eq!(
            validate_package_name("@my-org/my-pkg"),
            Ok("@my-org/my-pkg".to_string())
        );
    }

    // --- invalid names ---

    #[test]
    fn rejects_empty() {
        assert_eq!(validate_package_name(""), Err(ValidationError::Empty));
    }

    #[test]
    fn rejects_too_long() {
        let long = "a".repeat(215);
        assert_eq!(validate_package_name(&long), Err(ValidationError::TooLong));
    }

    #[test]
    fn accepts_exactly_214_chars() {
        let name = "a".repeat(214);
        assert!(validate_package_name(&name).is_ok());
    }

    #[test]
    fn rejects_leading_dot() {
        assert_eq!(
            validate_package_name(".hidden"),
            Err(ValidationError::InvalidStart)
        );
    }

    #[test]
    fn rejects_leading_underscore() {
        assert_eq!(
            validate_package_name("_private"),
            Err(ValidationError::InvalidStart)
        );
    }

    #[test]
    fn rejects_space() {
        assert!(matches!(
            validate_package_name("my package"),
            Err(ValidationError::InvalidCharacters(_))
        ));
    }

    #[test]
    fn rejects_uppercase() {
        assert_eq!(
            validate_package_name("MyPackage"),
            Err(ValidationError::UppercaseLetters)
        );
    }

    #[test]
    fn rejects_special_chars() {
        assert!(matches!(
            validate_package_name("pkg!name"),
            Err(ValidationError::InvalidCharacters(_))
        ));
    }

    #[test]
    fn rejects_scoped_missing_slash() {
        assert!(matches!(
            validate_package_name("@scope"),
            Err(ValidationError::MalformedScope(_))
        ));
    }

    #[test]
    fn rejects_scoped_empty_scope() {
        assert!(matches!(
            validate_package_name("@/name"),
            Err(ValidationError::MalformedScope(_))
        ));
    }

    #[test]
    fn rejects_scoped_empty_name() {
        assert!(matches!(
            validate_package_name("@scope/"),
            Err(ValidationError::MalformedScope(_))
        ));
    }

    #[test]
    fn rejects_scoped_scope_leading_dot() {
        assert!(matches!(
            validate_package_name("@.scope/name"),
            Err(ValidationError::MalformedScope(_))
        ));
    }

    #[test]
    fn rejects_scoped_name_leading_underscore() {
        assert!(matches!(
            validate_package_name("@scope/_name"),
            Err(ValidationError::MalformedScope(_))
        ));
    }
}
