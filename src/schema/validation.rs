use regex::Regex;
use std::sync::OnceLock;

static IDENTIFIER_REGEX: OnceLock<Regex> = OnceLock::new();

/// Validate that a string is a safe SQL identifier.
/// 
/// Allowed: `[a-zA-Z_][a-zA-Z0-9_]*`
pub fn validate_identifier(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Identifier cannot be empty".to_string());
    }

    let re = IDENTIFIER_REGEX.get_or_init(|| {
        Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap()
    });

    if !re.is_match(name) {
        return Err(format!("Invalid identifier '{}': must match [a-zA-Z_][a-zA-Z0-9_]*", name));
    }

    Ok(())
}

/// Quote an identifier for use in SQL.
/// 
/// Since we validate identifiers strictly, this is mostly for valid SQL syntax assurance
/// rather than escaping arbitrary strings (doubly safe).
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_identifiers() {
        assert!(validate_identifier("users").is_ok());
        assert!(validate_identifier("_hidden").is_ok());
        assert!(validate_identifier("table_123").is_ok());
        assert!(validate_identifier("camelCase").is_ok());
    }

    #[test]
    fn test_invalid_identifiers() {
        assert!(validate_identifier("").is_err());
        assert!(validate_identifier("123table").is_err()); // Cannot start with digit
        assert!(validate_identifier("table-name").is_err()); // No hyphens
        assert!(validate_identifier("table; DROP").is_err());
    }

    #[test]
    fn test_quoting() {
        assert_eq!(quote_identifier("users"), "\"users\"");
    }
}
