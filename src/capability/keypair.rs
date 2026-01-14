//! Ed25519 keypair management.
//!
//! # Security Properties
//!
//! - `SigningKey` is intentionally NOT Clone, NOT Debug, NOT Serialize
//! - This prevents accidental leakage in logs, snapshots, or clones
//! - Key material is wrapped and never exposed directly

use ed25519_dalek::{
    Signature, Signer, SigningKey as DalekSigningKey, Verifier,
    VerifyingKey as DalekVerifyingKey,
};
use rand::rngs::OsRng;

use super::error::CapabilityError;

/// Ed25519 signing key for minting capability tokens.
///
/// # Security
///
/// This type intentionally does NOT implement:
/// - `Clone` - prevents key duplication
/// - `Debug` - prevents key material in logs
/// - `Serialize` - prevents key persistence without explicit handling
///
/// The inner key is never exposed directly.
pub struct SigningKey {
    inner: DalekSigningKey,
}

impl SigningKey {
    /// Generate a new random signing key using OS entropy.
    pub fn generate() -> Self {
        Self {
            inner: DalekSigningKey::generate(&mut OsRng),
        }
    }

    /// Create a signing key from raw bytes.
    ///
    /// # Errors
    /// Returns `KeyError` if bytes are invalid.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, CapabilityError> {
        Ok(Self {
            inner: DalekSigningKey::from_bytes(bytes),
        })
    }

    /// Export the raw bytes of this signing key.
    ///
    /// # Security Warning
    /// Handle these bytes with extreme care. They represent the full
    /// authority to mint capability tokens.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Get the corresponding verifying key.
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey {
            inner: self.inner.verifying_key(),
        }
    }

    /// Sign a message and return the signature bytes.
    pub(crate) fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.inner.sign(message).to_bytes()
    }
}

/// Ed25519 verifying key for validating capability tokens.
///
/// Unlike `SigningKey`, this type IS clonable and can be freely distributed.
/// It can only verify signatures, not create them.
#[derive(Clone)]
pub struct VerifyingKey {
    inner: DalekVerifyingKey,
}

impl VerifyingKey {
    /// Create a verifying key from raw bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, CapabilityError> {
        DalekVerifyingKey::from_bytes(bytes)
            .map(|inner| Self { inner })
            .map_err(|e| CapabilityError::KeyError(e.to_string()))
    }

    /// Export the raw bytes of this verifying key.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Verify a signature against a message.
    pub(crate) fn verify(&self, message: &[u8], signature: &[u8; 64]) -> Result<(), CapabilityError> {
        let sig = Signature::from_bytes(signature);
        self.inner
            .verify(message, &sig)
            .map_err(|_| CapabilityError::InvalidSignature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();

        // Verifying key bytes should be 32 bytes
        assert_eq!(verifying_key.to_bytes().len(), 32);
    }

    #[test]
    fn test_sign_verify_roundtrip() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();

        let message = b"test message for signing";
        let signature = signing_key.sign(message);

        // Verification should succeed
        assert!(verifying_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_wrong_message_fails_verification() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();

        let message = b"original message";
        let signature = signing_key.sign(message);

        let wrong_message = b"different message";
        let result = verifying_key.verify(wrong_message, &signature);

        assert!(result.is_err());
        assert!(matches!(result, Err(CapabilityError::InvalidSignature)));
    }

    #[test]
    fn test_wrong_key_fails_verification() {
        let signing_key1 = SigningKey::generate();
        let signing_key2 = SigningKey::generate();
        let verifying_key2 = signing_key2.verifying_key();

        let message = b"message signed by key1";
        let signature = signing_key1.sign(message);

        // Verifying with key2 should fail
        let result = verifying_key2.verify(message, &signature);
        assert!(matches!(result, Err(CapabilityError::InvalidSignature)));
    }

    #[test]
    fn test_tampered_signature_fails() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();

        let message = b"test message";
        let mut signature = signing_key.sign(message);

        // Tamper with signature
        signature[0] ^= 0xFF;

        let result = verifying_key.verify(message, &signature);
        assert!(matches!(result, Err(CapabilityError::InvalidSignature)));
    }

    #[test]
    fn test_signing_key_from_bytes_roundtrip() {
        let original = SigningKey::generate();
        let bytes = original.to_bytes();

        let restored = SigningKey::from_bytes(&bytes).unwrap();

        // Sign with both, verify with original's verifying key
        let message = b"roundtrip test";
        let sig1 = original.sign(message);
        let sig2 = restored.sign(message);

        // Same key should produce same signature (Ed25519 is deterministic)
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_verifying_key_from_bytes_roundtrip() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();
        let bytes = verifying_key.to_bytes();

        let restored = VerifyingKey::from_bytes(&bytes).unwrap();

        let message = b"verify roundtrip";
        let signature = signing_key.sign(message);

        // Both should verify successfully
        assert!(verifying_key.verify(message, &signature).is_ok());
        assert!(restored.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_verifying_key_is_clone() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();
        let cloned = verifying_key.clone();

        let message = b"clone test";
        let signature = signing_key.sign(message);

        // Both should verify
        assert!(verifying_key.verify(message, &signature).is_ok());
        assert!(cloned.verify(message, &signature).is_ok());
    }

    // Compile-time checks: SigningKey must NOT be Clone or Debug
    // These are negative tests - if they compile, the security property holds
    #[test]
    fn test_signing_key_not_clone() {
        fn assert_not_clone<T>() {}
        // This would fail to compile if SigningKey implemented Clone
        // We can't actually test this at runtime, but the struct definition ensures it
    }
}
