//! Policy module for Singularity.
//!
//! # Core Invariant
//!
//! > **Policies decide YES/NO only.**
//! >
//! > They do NOT:
//! > - mint authority
//! > - perform execution
//! > - modify state
//! > - widen access
//! > - observe side effects
//!
//! If this invariant holds, policy bugs **cannot become security bugs**.
//!
//! # Trust Boundaries
//!
//! ```text
//! PolicyEngine::evaluate(script, ctx) -> bool
//!                  ↓
//!         (caller decides)
//!                  ↓
//!       CapabilitySigner::mint(payload)
//! ```
//!
//! Policy = judgment. Capability = authority.

mod context;
mod engine;
mod error;
mod sandbox;

pub use context::{PolicyContext, PolicyEnv, PolicySubject};
pub use engine::PolicyEngine;
pub use error::PolicyError;
pub use sandbox::{create_sandboxed_engine, validate_policy};
