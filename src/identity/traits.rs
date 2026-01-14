//! Identity verifier trait - the identity contract.
//!
//! # Invariant
//!
//! > **Identity verification produces `PolicySubject`, nothing more.**
//!
//! No authority is granted here.
//! No capability is minted here.
//! No execution is reachable here.

use crate::policy::PolicySubject;
use super::error::IdentityError;

/// Identity verifier trait.
///
/// All identity backends (JWT, OIDC, LDAP, etc.) implement this trait.
/// The only output is `PolicySubject` - a normalized identity.
pub trait IdentityVerifier: Send + Sync {
    /// Verify an identity token and produce a PolicySubject.
    ///
    /// # Arguments
    ///
    /// * `token` - The raw identity token (e.g., JWT string)
    /// * `now` - Current timestamp (Unix epoch seconds) for expiration checks
    ///
    /// # Returns
    ///
    /// `PolicySubject` on success, `IdentityError` on failure.
    fn verify(&self, token: &str, now: u64) -> Result<PolicySubject, IdentityError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check: IdentityVerifier must be Send + Sync
    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_identity_verifier_is_send_sync() {
        fn check_impl<T: IdentityVerifier>() {
            _assert_send_sync::<T>();
        }
    }
}
