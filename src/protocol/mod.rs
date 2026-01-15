//! Protocol module for Singularity capability-based execution engine.
//!
//! Defines the three core message types:
//! - `OpRequest` - Intent declaration (does NOT mutate state)
//! - `CapGrant` - Capability grant response containing signed token
//! - `OpExecute` - Execution request (consumes opaque capability token)

mod execute;
mod grant;
pub mod opcode;
mod request;
mod types;

pub use execute::OpExecute;
pub use grant::{CapGrant, CapabilityPayload, CapabilityToken};
pub use opcode::Opcode;
pub use request::OpRequest;
pub use types::{CapabilityBinding, FieldSet, Resource};
