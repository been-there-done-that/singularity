//! Singularity - Capability-Based Execution Engine
//!
//! A single-binary, embedded-first backend primitive designed to be linked
//! into any application (server, desktop, CLI, embedded).
//!
//! # Core Invariant
//!
//! **NO state-changing operation may execute without a valid,
//! cryptographically verified Capability Token.**
//!
//! # Pipeline
//!
//! Every request follows this mandatory pipeline:
//!
//! 1. **Identity Verification** - Verify OIDC JWT or LDAP authentication
//! 2. **Policy Evaluation** - Evaluate intent against policy rules
//! 3. **Capability Issuance** - Issue signed, short-lived capability token
//! 4. **Execution** - Execute operation with verified capability
//!
//! # Protocol
//!
//! Three message types:
//! - [`protocol::OpRequest`] - Intent declaration (does NOT mutate state)
//! - [`protocol::CapGrant`] - Capability grant response
//! - [`protocol::OpExecute`] - Execution request (requires valid capability)

pub mod capability;
pub mod execution;
pub mod identity;
pub mod policy;
pub mod protocol;
pub mod state;
