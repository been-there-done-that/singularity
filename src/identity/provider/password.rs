//! Password hashing with Argon2id.
//!
//! # Security
//!
//! - Uses Argon2id (memory-hard, GPU-resistant)
//! - Salt generated per-password
//! - Never stores plaintext

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("hashing failed: {0}")]
    HashingFailed(String),
    #[error("verification failed")]
    VerificationFailed,
    #[error("invalid hash format")]
    InvalidHashFormat,
}

/// Hash a password using Argon2id.
///
/// Returns a PHC-format string containing the hash and salt.
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| PasswordError::HashingFailed(e.to_string()))?;

    Ok(hash.to_string())
}

/// Verify a password against a stored hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, PasswordError> {
    let parsed_hash =
        PasswordHash::new(hash).map_err(|_| PasswordError::InvalidHashFormat)?;

    let argon2 = Argon2::default();

    match argon2.verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let password = "my_secure_password";
        let hash = hash_password(password).unwrap();

        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_different_hashes() {
        let password = "same_password";
        let hash1 = hash_password(password).unwrap();
        let hash2 = hash_password(password).unwrap();

        // Salts are unique, so hashes should differ
        assert_ne!(hash1, hash2);

        // But both should verify
        assert!(verify_password(password, &hash1).unwrap());
        assert!(verify_password(password, &hash2).unwrap());
    }
}
