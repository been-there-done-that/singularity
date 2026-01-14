//! State module for Singularity.
//!
//! # Core Principle
//!
//! > **State backends are execution backends, NOT database abstractions.**
//!
//! Singularity does NOT become: ORM, query builder, SQL abstraction.
//!
//! # Frozen Contract
//!
//! The `State` trait is FROZEN. All backends implement it.
//! SQLite is the reference implementation - other backends must match its semantics.
//!
//! # Architecture
//!
//! ```text
//! Execution
//!     │
//!     ▼
//! State trait (frozen)
//!     │
//!     ├── SqliteState (reference)
//!     ├── PostgresState (future)
//!     └── DuckDbState (future)
//! ```

mod capabilities;
mod conformance;
mod error;
pub mod sqlite;
mod traits;

pub use capabilities::StateCapabilities;
pub use conformance::run_conformance_tests;
pub use error::StateError;
pub use sqlite::SqliteState;
pub use traits::State;
