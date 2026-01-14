//! Capability token signing (payload → token).
//!
//! # Token Format
//!
//! Tokens use a tagged CBOR envelope for safety and future-proofing:
//!
//! ```text
//! base64(cbor({
//!     1: payload_bytes,   // canonical CBOR-encoded CapabilityPayload
//!     2: signature_bytes  // Ed25519 signature over payload_bytes
//! }))
//! ```
//!
//! This format:
//! - Avoids length-ambiguity attacks
//! - Supports future versioning
//! - Remains opaque on the wire

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

use super::error::CapabilityError;
use super::keypair::SigningKey;
use crate::protocol::{CapabilityPayload, CapabilityToken};

/// Token envelope structure (internal, never exposed).
#[derive(Serialize, Deserialize)]
struct TokenEnvelope {
    #[serde(rename = "1")]
    payload: Vec<u8>,
    #[serde(rename = "2")]
    signature: Vec<u8>,
}

/// Capability signer for minting tokens.
///
/// Holds a signing key and produces opaque capability tokens from payloads.
pub struct CapabilitySigner {
    signing_key: SigningKey,
}

impl CapabilitySigner {
    /// Create a new signer with the given signing key.
    pub fn new(signing_key: SigningKey) -> Self {
        Self { signing_key }
    }

    /// Get the verifying key corresponding to this signer.
    pub fn verifying_key(&self) -> super::keypair::VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Mint a capability token from a payload.
    ///
    /// # Process
    /// 1. Serialize payload to canonical CBOR
    /// 2. Sign the CBOR bytes with Ed25519
    /// 3. Create tagged envelope with payload + signature
    /// 4. Base64-encode the envelope
    ///
    /// # Errors
    /// Returns `EncodingError` if CBOR serialization fails.
    pub fn mint(&self, payload: &CapabilityPayload) -> Result<CapabilityToken, CapabilityError> {
        // 1. Serialize payload to canonical CBOR
        let payload_bytes = payload
            .to_bytes()
            .map_err(|e| CapabilityError::EncodingError(format!("failed to encode payload: {}", e)))?;

        // 2. Sign the payload bytes
        let signature = self.signing_key.sign(&payload_bytes);

        // 3. Create tagged envelope
        let envelope = TokenEnvelope {
            payload: payload_bytes,
            signature: signature.to_vec(),
        };

        // 4. Serialize envelope to CBOR
        let mut envelope_bytes = Vec::new();
        ciborium::into_writer(&envelope, &mut envelope_bytes)
            .map_err(|e| CapabilityError::EncodingError(format!("failed to encode envelope: {}", e)))?;

        // 5. Base64-encode
        let token_string = URL_SAFE_NO_PAD.encode(&envelope_bytes);

        Ok(CapabilityToken::new(token_string))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{FieldSet, Resource};

    fn create_test_payload() -> CapabilityPayload {
        CapabilityPayload::new(
            "cap-test-001",
            "resource.read",
            Resource::instance("user", "123"),
            FieldSet::new(["name", "email"]),
            1704067200,
            1704067260,
        )
    }

    #[test]
    fn test_mint_produces_non_empty_token() {
        let signing_key = SigningKey::generate();
        let signer = CapabilitySigner::new(signing_key);

        let payload = create_test_payload();
        let token = signer.mint(&payload).unwrap();

        assert!(!token.as_str().is_empty());
    }

    #[test]
    fn test_mint_produces_base64_token() {
        let signing_key = SigningKey::generate();
        let signer = CapabilitySigner::new(signing_key);

        let payload = create_test_payload();
        let token = signer.mint(&payload).unwrap();

        // Should be valid base64 (URL-safe, no padding)
        let decode_result = URL_SAFE_NO_PAD.decode(token.as_str());
        assert!(decode_result.is_ok());
    }

    #[test]
    fn test_mint_produces_decodable_envelope() {
        let signing_key = SigningKey::generate();
        let signer = CapabilitySigner::new(signing_key);

        let payload = create_test_payload();
        let token = signer.mint(&payload).unwrap();

        // Decode base64
        let envelope_bytes = URL_SAFE_NO_PAD.decode(token.as_str()).unwrap();

        // Parse CBOR envelope
        let envelope: TokenEnvelope = ciborium::from_reader(&envelope_bytes[..]).unwrap();

        // Envelope should contain payload and signature
        assert!(!envelope.payload.is_empty());
        assert_eq!(envelope.signature.len(), 64); // Ed25519 signature is 64 bytes
    }

    #[test]
    fn test_mint_payload_is_recoverable() {
        let signing_key = SigningKey::generate();
        let signer = CapabilitySigner::new(signing_key);

        let payload = create_test_payload();
        let token = signer.mint(&payload).unwrap();

        // Decode and extract payload
        let envelope_bytes = URL_SAFE_NO_PAD.decode(token.as_str()).unwrap();
        let envelope: TokenEnvelope = ciborium::from_reader(&envelope_bytes[..]).unwrap();

        // Deserialize payload
        let recovered: CapabilityPayload =
            ciborium::from_reader(&envelope.payload[..]).unwrap();

        assert_eq!(recovered.cap_id, payload.cap_id);
        assert_eq!(recovered.op.as_str(), payload.op.as_str());
        assert_eq!(recovered.resource, payload.resource);
    }

    #[test]
    fn test_mint_signature_is_valid() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();
        let signer = CapabilitySigner::new(signing_key);

        let payload = create_test_payload();
        let token = signer.mint(&payload).unwrap();

        // Extract envelope
        let envelope_bytes = URL_SAFE_NO_PAD.decode(token.as_str()).unwrap();
        let envelope: TokenEnvelope = ciborium::from_reader(&envelope_bytes[..]).unwrap();

        // Verify signature
        let signature: [u8; 64] = envelope.signature.try_into().unwrap();
        let result = verifying_key.verify(&envelope.payload, &signature);
        assert!(result.is_ok());
    }

    #[test]
    fn test_different_payloads_produce_different_tokens() {
        let signing_key = SigningKey::generate();
        let signer = CapabilitySigner::new(signing_key);

        let payload1 = CapabilityPayload::new(
            "cap-001",
            "resource.read",
            Resource::instance("user", "123"),
            FieldSet::all(),
            1704067200,
            1704067260,
        );

        let payload2 = CapabilityPayload::new(
            "cap-002", // Different ID
            "resource.read",
            Resource::instance("user", "123"),
            FieldSet::all(),
            1704067200,
            1704067260,
        );

        let token1 = signer.mint(&payload1).unwrap();
        let token2 = signer.mint(&payload2).unwrap();

        assert_ne!(token1.as_str(), token2.as_str());
    }

    #[test]
    fn test_same_payload_same_signer_produces_same_token() {
        let signing_key = SigningKey::generate();
        let signer = CapabilitySigner::new(signing_key);

        let payload = create_test_payload();
        let token1 = signer.mint(&payload).unwrap();
        let token2 = signer.mint(&payload).unwrap();

        // Ed25519 is deterministic, so same payload = same token
        assert_eq!(token1.as_str(), token2.as_str());
    }

    #[test]
    fn test_different_signers_produce_different_signatures() {
        let signer1 = CapabilitySigner::new(SigningKey::generate());
        let signer2 = CapabilitySigner::new(SigningKey::generate());

        let payload = create_test_payload();
        let token1 = signer1.mint(&payload).unwrap();
        let token2 = signer2.mint(&payload).unwrap();

        // Different keys should produce different tokens
        assert_ne!(token1.as_str(), token2.as_str());
    }
}
