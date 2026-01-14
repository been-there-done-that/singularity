//! State trait - the FROZEN kernel contract.
//!
//! # Invariant
//!
//! > **This is the ONLY interface execution uses for state.**
//!
//! Backends (SQLite, Postgres, etc.) implement this trait.
//! Execution NEVER knows which backend is active.
//!
//! # Stability
//!
//! This trait is FROZEN. Changes require:
//! - Major version bump
//! - Security review
//! - All conformance tests updated

use crate::execution::ExecutionTarget;
use crate::protocol::FieldSet;

use super::capabilities::StateCapabilities;
use super::error::StateError;

/// The semantic state interface.
///
/// All state backends (SQLite, Postgres, DuckDB, etc.) implement this trait.
/// Execution code interacts ONLY through this interface.
///
/// # Design Principles
///
/// - **Semantic, not query-shaped**: No SQL, no predicates, no expressions
/// - **Explicit inputs**: Target, fields, constraints all passed explicitly
/// - **Capability-aware**: Backends advertise what they support
/// - **Backend-agnostic**: Execution cannot detect which backend is active
///
/// # Methods
///
/// - `read()`: Fetch resource(s) matching target + constraints
/// - `write()`: Create or update resource (unified, execution decides semantics)
/// - `delete()`: Remove resource(s) matching target + constraints
/// - `capabilities()`: Get backend capabilities
pub trait State: Send + Sync {
    /// Read resource(s) matching target and constraints.
    ///
    /// # Arguments
    ///
    /// * `target` - Resource to read (instance or collection)
    /// * `fields` - Fields to return (already validated by execution)
    /// * `constraints` - Optional CAS-style constraints
    ///
    /// # Returns
    ///
    /// * Single object for instance target
    /// * Array for collection target
    /// * Error if not found or constraints fail
    fn read(
        &self,
        target: &ExecutionTarget,
        fields: &FieldSet,
        constraints: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, StateError>;

    /// Write (create or update) a resource.
    ///
    /// # Semantics
    ///
    /// - If `target.resource.resource_id` is `Some` → update existing
    /// - If `target.resource.resource_id` is `None` → create new
    /// - State decides internal representation
    /// - Execution decides whether to allow
    ///
    /// # Arguments
    ///
    /// * `target` - Resource to write
    /// * `fields` - Fields being written (already validated by execution)
    /// * `payload` - Data to write
    /// * `constraints` - Optional CAS-style constraints
    ///
    /// # Returns
    ///
    /// Number of affected rows/records.
    fn write(
        &self,
        target: &ExecutionTarget,
        fields: &FieldSet,
        payload: &serde_json::Value,
        constraints: Option<&serde_json::Value>,
    ) -> Result<u64, StateError>;

    /// Delete resource(s) matching target and constraints.
    ///
    /// # Arguments
    ///
    /// * `target` - Resource(s) to delete
    /// * `constraints` - Optional CAS-style constraints
    ///
    /// # Returns
    ///
    /// Number of deleted rows/records.
    fn delete(
        &self,
        target: &ExecutionTarget,
        constraints: Option<&serde_json::Value>,
    ) -> Result<u64, StateError>;

    /// Execute internal DDL (migrations).
    /// Safe only because it is internal.
    fn execute_ddl(&self, sql: &str) -> Result<(), StateError>;

    /// Get the capabilities of this state backend.
    fn capabilities(&self) -> &StateCapabilities;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check: State must be Send + Sync
    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_state_trait_is_send_sync() {
        // This is a compile-time test - if it compiles, the trait bounds are correct
        fn check_impl<T: State>() {
            _assert_send_sync::<T>();
        }
        // The test passes if this module compiles
    }
}
