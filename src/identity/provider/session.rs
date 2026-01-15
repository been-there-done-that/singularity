//! Session management - cryptographic session keys.
//!
//! # Design
//!
//! Session = Authority Anchor (long-lived)
//! JWT = Capability Token (short-lived projection of session)
//!
//! # Security Properties
//!
//! - Session key is 32 random bytes (256 bits)
//! - SHA256 hash stored in DB (never raw key)
//! - `skh` claim in JWT binds token to session (proof of possession)
//! - Revocation is instant (check revoked_at on every request)
//!
//! # Rules
//!
//! This module is PURE:
//! - No DB access
//! - No IO
//! - No global state
//! - Deterministic, testable, panic-free

use sha2::{Sha256, Digest};
use rand::RngCore;

/// Session key length in bytes (256 bits).
const SESSION_KEY_LENGTH: usize = 32;

/// Generate a new random session key.
/// 
/// Returns hex-encoded string (64 characters).
pub fn generate_session_key() -> String {
    let mut key = [0u8; SESSION_KEY_LENGTH];
    rand::thread_rng().fill_bytes(&mut key);
    hex::encode(key)
}

/// Hash a session key using SHA256.
/// 
/// This is what gets stored in DB and embedded in JWT as `skh`.
pub fn hash_session_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Verify that a session key matches a stored hash.
pub fn verify_session_key(key: &str, stored_hash: &str) -> bool {
    hash_session_key(key) == stored_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_session_key_length() {
        let key = generate_session_key();
        // 32 bytes * 2 hex chars = 64 chars
        assert_eq!(key.len(), 64);
    }

    #[test]
    fn test_generate_session_key_unique() {
        let key1 = generate_session_key();
        let key2 = generate_session_key();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_hash_session_key_deterministic() {
        let key = "test-session-key";
        let hash1 = hash_session_key(key);
        let hash2 = hash_session_key(key);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_session_key_length() {
        let key = "test-session-key";
        let hash = hash_session_key(key);
        // SHA256 = 32 bytes * 2 hex chars = 64 chars
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_verify_session_key_valid() {
        let key = generate_session_key();
        let hash = hash_session_key(&key);
        assert!(verify_session_key(&key, &hash));
    }

    #[test]
    fn test_verify_session_key_invalid() {
        let key = generate_session_key();
        let hash = hash_session_key(&key);
        let other_key = generate_session_key();
        assert!(!verify_session_key(&other_key, &hash));
    }
}
