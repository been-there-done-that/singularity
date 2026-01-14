//! Execution module for Singularity.
//!
//! # Core Invariant
//!
//! > **All execution must be driven exclusively by VerifiedCapability.**
//!
//! - Reads may return ONLY authorized fields
//! - Writes must reference ONLY authorized fields (rejected, not silently dropped)
//! - Resource identity must match exactly
//! - Constraints must be validated before mutation
//! - No request metadata may influence authorization
//!
//! # Separation of Concerns
//!
//! - `ExecutionContext` = **authority** (from verified capability)
//! - `ExecutionMeta` = **observability** (request metadata for logging)
//! - `ExecutionTarget` = **explicit resource** (not inferred from payload)
//!
//! # Execution Order
//!
//! 1. Resource target validation
//! 2. Constraint validation (CAS-style)
//! 3. Field validation (for writes)
//! 4. Mutation
//! 5. Output filtering (for reads)

mod constraint;
mod context;
mod executor;
mod result;

pub use constraint::validate_constraints;
pub use context::{ExecutionContext, ExecutionMeta, ExecutionTarget};
pub use executor::OperationExecutor;
pub use result::{ExecutionError, ExecutionResult};
