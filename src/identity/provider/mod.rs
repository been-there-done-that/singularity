//! Identity Provider - JWT issuance for authenticated users.
//!
//! # Core Principle
//!
//! > **Identity issuance produces JWTs. Kernel verifies them the same way.**
//!
//! This module:
//! - Owns credentials
//! - Issues JWTs
//! - Does NOT bypass policy
//! - Does NOT touch capabilities

pub mod jwt;
pub mod password;
pub mod service;

pub use jwt::JwtIssuer;
pub use password::{hash_password, verify_password};
pub use service::{IdentityService, AuthError, AuthResponse, LoginRequest, RegisterRequest};

