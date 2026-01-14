//! State capabilities.
//!
//! Backends advertise their capabilities explicitly.
//! Execution can require or reject based on these.

/// Advertised capabilities of a state backend.
///
/// This prevents silent feature degradation.
/// If a capability is `false`, execution can explicitly fail.
#[derive(Debug, Clone)]
pub struct StateCapabilities {
    /// Supports atomic transactions.
    pub transactions: bool,
    /// Supports CAS-style constraints (version_eq, not_deleted, etc.).
    pub cas_constraints: bool,
    /// Supports JSON/document state natively.
    pub json_storage: bool,
    /// Supports partial field updates (vs full replace).
    pub partial_updates: bool,
    /// Backend identifier (for logging/debugging only).
    pub backend_name: &'static str,
}

impl StateCapabilities {
    /// Create capabilities for SQLite backend.
    pub fn sqlite() -> Self {
        Self {
            transactions: true,
            cas_constraints: true,
            json_storage: true,  // via JSON1 extension
            partial_updates: true,
            backend_name: "sqlite",
        }
    }

    /// Create capabilities for an in-memory test backend.
    pub fn in_memory() -> Self {
        Self {
            transactions: false,
            cas_constraints: true,
            json_storage: true,
            partial_updates: true,
            backend_name: "in_memory",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_capabilities() {
        let caps = StateCapabilities::sqlite();
        assert!(caps.transactions);
        assert!(caps.cas_constraints);
        assert_eq!(caps.backend_name, "sqlite");
    }
}
