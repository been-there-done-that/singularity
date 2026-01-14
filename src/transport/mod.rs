//! Transport layer: HTTP/gRPC adapters only.
//!
//! **Invariant**: Transport is a dumb pipe. No logic.

pub mod error;
pub mod http;

pub use error::TransportError;
