//! Transport layer: HTTP/gRPC adapters only.
//!
//! **Invariant**: Transport is a dumb pipe. No logic.

pub mod error;
pub mod grpc;
pub mod http;
pub mod pipeline;
pub mod state;

pub use error::TransportError;
pub use state::AppState;
