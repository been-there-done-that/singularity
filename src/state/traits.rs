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

use crate::execution::{ExecutionTarget, ExecutionResult};
use crate::protocol::FieldSet;
use crate::planner::{LogicalPlan, SchemaView};
use crate::protocol::data::PlanGrant;


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
pub trait State: SchemaView + Send + Sync {
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

    /// Ensure an internal user exists for the given external subject.
    ///
    /// # Arguments
    ///
    /// * `external_subject` - Value from JWT "sub" claim
    /// * `roles` - List of roles from JWT
    ///
    /// # Returns
    ///
    /// * The persistent internal ID for this user.
    fn ensure_internal_user(&self, external_subject: &str, roles: &[String]) -> Result<String, StateError>;

    /// Get the owner_id of a resource instance.
    /// Returns None if resource doesn't exist or doesn't have an owner.
    fn get_resource_owner(&self, resource_type: &str, resource_id: &str) -> Result<Option<String>, StateError>;

    /// Get the capabilities of this state backend.
    fn capabilities(&self) -> &StateCapabilities;

    /// Get current schema version.
    /// Returns None if version table doesn't exist.
    fn get_schema_version(&self) -> Result<Option<u64>, StateError>;

    /// Count resource(s) matching target and constraints.
    ///
    /// # Arguments
    ///
    /// * `target` - Resource to count
    /// * `constraints` - Optional CAS-style constraints
    ///
    /// # Returns
    ///
    /// * Number of matching resources
    fn count(
        &self,
        target: &ExecutionTarget,
        constraints: Option<&serde_json::Value>,
    ) -> Result<u64, StateError>;

    /// Rename a table (model).
    ///
    /// Used for schema evolution. Backends should support this atomically.
    fn rename_table(&self, old_name: &str, new_name: &str) -> Result<(), StateError>;

    /// Rename a column (field).
    ///
    /// Used for schema evolution. Backends should support this atomically.
    fn rename_column(&self, table: &str, old_col: &str, new_col: &str) -> Result<(), StateError>;

    /// Execute a logical plan using the backend's native execution engine.
    ///
    /// This enables the PAP pipeline (Plan-Auth-Plan execution).
    fn execute_plan(
        &self,
        plan: &LogicalPlan,
        grant: &PlanGrant,
        subject_id: Option<&str>,
    ) -> Result<ExecutionResult, StateError>;

    // =========================================================================
    // Access Profile CRUD (Admin-only, Control-plane)
    // =========================================================================

    /// Create an access profile.
    fn create_access_profile(
        &self,
        input: &crate::protocol::data::AccessProfileInput,
        now: u64,
    ) -> Result<String, StateError>;

    /// List access profiles for a model.
    fn list_access_profiles(
        &self,
        model_id: &str,
    ) -> Result<Vec<crate::protocol::data::AccessProfileOutput>, StateError>;

    /// Get a specific access profile.
    fn get_access_profile(
        &self,
        id: &str,
    ) -> Result<Option<crate::protocol::data::AccessProfileOutput>, StateError>;

    /// Update an access profile.
    fn update_access_profile(
        &self,
        id: &str,
        updates: &crate::protocol::data::AccessProfileUpdate,
        now: u64,
    ) -> Result<(), StateError>;

    /// Delete an access profile.
    fn delete_access_profile(&self, id: &str) -> Result<(), StateError>;

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
