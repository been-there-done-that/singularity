//! Sandboxed Rhai engine configuration.
//!
//! # Security Properties
//!
//! The sandbox enforces:
//! - No loops (while, loop, for)
//! - No function definitions (fn)
//! - Limited recursion depth
//! - Limited operations count
//! - Limited memory usage

use rhai::Engine;
use super::error::PolicyError;

/// Maximum operations before timeout.
const MAX_OPERATIONS: u64 = 10_000;

/// Maximum expression depth.
const MAX_EXPR_DEPTH: usize = 32;

/// Maximum call stack depth.
const MAX_CALL_LEVELS: usize = 8;

/// Maximum string length in bytes.
const MAX_STRING_SIZE: usize = 4096;

/// Maximum array size.
const MAX_ARRAY_SIZE: usize = 256;

/// Maximum map size.
const MAX_MAP_SIZE: usize = 128;

/// Create a sandboxed Rhai engine with strict security limits.
///
/// This engine is suitable for evaluating untrusted policy scripts.
///
/// # Disabled Features
///
/// - `loop`, `while`, `for` - prevent infinite loops
/// - `fn` - prevent function definitions
/// - No file I/O, network, or system access (Rhai default)
///
/// # Limits
///
/// - Operations: 10,000 max
/// - Expression depth: 32
/// - Call stack: 8 levels
/// - Strings: 4KB max
/// - Arrays: 256 elements max
/// - Maps: 128 entries max
pub fn create_sandboxed_engine() -> Engine {
    let mut engine = Engine::new();

    // Set resource limits
    engine.set_max_operations(MAX_OPERATIONS);
    engine.set_max_expr_depths(MAX_EXPR_DEPTH, MAX_EXPR_DEPTH);
    engine.set_max_call_levels(MAX_CALL_LEVELS);
    engine.set_max_string_size(MAX_STRING_SIZE);
    engine.set_max_array_size(MAX_ARRAY_SIZE);
    engine.set_max_map_size(MAX_MAP_SIZE);

    // Disable looping constructs
    engine.disable_symbol("loop");
    engine.disable_symbol("while");
    engine.disable_symbol("for");

    // Disable function definitions (policies are expressions)
    engine.disable_symbol("fn");

    // Disable potentially dangerous features
    engine.disable_symbol("eval");

    engine
}

/// Validate a policy script for syntax errors without executing.
pub fn validate_policy(engine: &Engine, script: &str) -> Result<(), PolicyError> {
    engine
        .compile(script)
        .map(|_| ())
        .map_err(|e| PolicyError::InvalidPolicy(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_blocks_while_loop() {
        let engine = create_sandboxed_engine();
        let script = "let x = 0; while x < 10 { x += 1; } x";
        let result = validate_policy(&engine, script);
        assert!(result.is_err());
    }

    #[test]
    fn test_sandbox_blocks_for_loop() {
        let engine = create_sandboxed_engine();
        let script = "let sum = 0; for x in [1, 2, 3] { sum += x; } sum";
        let result = validate_policy(&engine, script);
        assert!(result.is_err());
    }

    #[test]
    fn test_sandbox_blocks_loop() {
        let engine = create_sandboxed_engine();
        let script = "loop { break; }";
        let result = validate_policy(&engine, script);
        assert!(result.is_err());
    }

    #[test]
    fn test_sandbox_blocks_fn_definition() {
        let engine = create_sandboxed_engine();
        let script = "fn foo() { true } foo()";
        let result = validate_policy(&engine, script);
        assert!(result.is_err());
    }

    #[test]
    fn test_sandbox_blocks_eval() {
        let engine = create_sandboxed_engine();
        let script = r#"eval("true")"#;
        let result = validate_policy(&engine, script);
        assert!(result.is_err());
    }

    #[test]
    fn test_sandbox_allows_simple_expressions() {
        let engine = create_sandboxed_engine();

        // Simple boolean expressions should work
        assert!(validate_policy(&engine, "true").is_ok());
        assert!(validate_policy(&engine, "1 + 2 == 3").is_ok());
        assert!(validate_policy(&engine, "let x = 5; x > 3").is_ok());
    }

    #[test]
    fn test_sandbox_allows_conditionals() {
        let engine = create_sandboxed_engine();
        let script = "if true { 1 } else { 2 }";
        assert!(validate_policy(&engine, script).is_ok());
    }

    #[test]
    fn test_sandbox_allows_array_operations() {
        let engine = create_sandboxed_engine();
        let script = r#"let arr = ["admin", "user"]; arr.contains("admin")"#;
        assert!(validate_policy(&engine, script).is_ok());
    }
}
