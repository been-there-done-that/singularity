//! Identity verification module.
//!
//! # Core Principle
//!
//! > **Identity verification answers "Who is this?" — nothing more.**
//!
//! - No authority is granted here
//! - No capability is minted here  
//! - No execution is reachable here
//!
//! # Pipeline Position
//!
//! ```text
//! JWT Token → Identity Module → PolicySubject → Policy Engine
//! ```
//!
//! # Key Invariant
//!
//! The ONLY output is `PolicySubject`. No raw claims leak.

mod claims;
mod error;
mod jwt;
mod traits;

pub use claims::{ClaimConfig, StandardClaims};
pub use error::IdentityError;
pub use jwt::{JwtKeySource, JwtVerifier};
pub use traits::IdentityVerifier;
