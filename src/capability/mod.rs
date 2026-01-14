//! Capability module for Singularity.
//!
//! This module is the **cryptographic core** of the system:
//!
//! - Signs `CapabilityPayload` → produces `CapabilityToken`
//! - Verifies tokens and enforces all security invariants
//!
//! # Security Properties
//!
//! - Ed25519 signatures (deterministic, fast, safe)
//! - Canonical CBOR encoding for signing
//! - Tagged envelope format for tokens
//! - Strict invariant enforcement

mod error;
mod keypair;
mod signer;
mod verifier;

pub use error::{CapabilityError, MAX_CAP_TTL_SECS, MAX_CLOCK_SKEW_SECS};
pub use keypair::{SigningKey, VerifyingKey};
pub use signer::CapabilitySigner;
pub use verifier::{CapabilityVerifier, VerifiedCapability};
