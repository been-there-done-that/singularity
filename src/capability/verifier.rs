//! Capability token verification and invariant enforcement.
//!
//! This module is the **security enforcement point** of the system.
//!
//! # Invariants Enforced
//!
//! - Signature validity (Ed25519)
//! - Token not expired
//! - Token not issued in the future (clock skew)
//! - TTL within bounds
//! - Operation scope match
//! - Resource match
//! - Field access authorization
//! - Client binding (nonce/IP) if specified

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

use super::error::{CapabilityError, MAX_CAP_TTL_SECS, MAX_CLOCK_SKEW_SECS};
use super::keypair::VerifyingKey;
use crate::protocol::{CapabilityPayload, CapabilityToken, FieldSet, Opcode, Resource};

/// Token envelope structure (matches signer's format).
#[derive(Serialize, Deserialize)]
struct TokenEnvelope {
    #[serde(rename = "1")]
    payload: Vec<u8>,
    #[serde(rename = "2")]
    signature: Vec<u8>,
}

/// A capability that has been cryptographically verified.
///
/// This type can ONLY be constructed by the `CapabilityVerifier`,
/// ensuring that any `VerifiedCapability` instance represents a
/// legitimately signed and valid capability.
///
/// The inner payload is private to prevent mutation or construction bypass.
pub struct VerifiedCapability {
    payload: CapabilityPayload,
}

impl VerifiedCapability {
    /// Get the capability ID.
    pub fn cap_id(&self) -> &str {
        &self.payload.cap_id
    }

    /// Get the authorized operation.
    pub fn op(&self) -> &Opcode {
        &self.payload.op
    }

    /// Get the authorized resource.
    pub fn resource(&self) -> &Resource {
        &self.payload.resource
    }

    /// Get the authorized fields.
    pub fn fields(&self) -> &FieldSet {
        &self.payload.fields
    }

    /// Get the issuance timestamp.
    pub fn issued_at(&self) -> u64 {
        self.payload.issued_at
    }

    /// Get the expiration timestamp.
    pub fn expires_at(&self) -> u64 {
        self.payload.expires_at
    }

    /// Get the full payload (read-only reference).
    pub fn payload(&self) -> &CapabilityPayload {
        &self.payload
    }
}

/// Capability verifier for validating tokens.
///
/// Holds a verifying key and validates token signatures and invariants.
pub struct CapabilityVerifier {
    verifying_key: VerifyingKey,
}

impl CapabilityVerifier {
    /// Create a new verifier with the given verifying key.
    pub fn new(verifying_key: VerifyingKey) -> Self {
        Self { verifying_key }
    }

    /// Verify a token and extract the payload.
    ///
    /// # Checks Performed
    /// - Base64 decoding
    /// - CBOR envelope parsing
    /// - Ed25519 signature verification
    /// - Expiry (token not expired)
    /// - Clock skew (token not issued in future)
    /// - TTL bounds (not exceeding MAX_CAP_TTL_SECS)
    ///
    /// # Errors
    /// Returns appropriate `CapabilityError` for any validation failure.
    pub fn verify(
        &self,
        token: &CapabilityToken,
        now: u64,
    ) -> Result<VerifiedCapability, CapabilityError> {
        // 1. Decode base64
        let envelope_bytes = URL_SAFE_NO_PAD
            .decode(token.as_str())
            .map_err(|e| CapabilityError::TokenMalformed(format!("invalid base64: {}", e)))?;

        // 2. Parse CBOR envelope
        let envelope: TokenEnvelope = ciborium::from_reader(&envelope_bytes[..])
            .map_err(|e| CapabilityError::TokenMalformed(format!("invalid envelope: {}", e)))?;

        // 3. Verify signature
        let signature: [u8; 64] = envelope
            .signature
            .try_into()
            .map_err(|_| CapabilityError::TokenMalformed("invalid signature length".to_string()))?;

        self.verifying_key.verify(&envelope.payload, &signature)?;

        // 4. Parse payload
        let payload: CapabilityPayload = ciborium::from_reader(&envelope.payload[..])
            .map_err(|e| CapabilityError::TokenMalformed(format!("invalid payload: {}", e)))?;

        // 5. Check clock skew (token not issued in future)
        if payload.issued_at > now + MAX_CLOCK_SKEW_SECS {
            return Err(CapabilityError::ClockSkew {
                now,
                issued_at: payload.issued_at,
            });
        }

        // 6. Check TTL bounds
        let ttl = payload.expires_at.saturating_sub(payload.issued_at);
        if ttl > MAX_CAP_TTL_SECS {
            return Err(CapabilityError::InvalidTTL {
                ttl_secs: ttl,
                max_ttl_secs: MAX_CAP_TTL_SECS,
            });
        }

        // 7. Check expiry
        if now >= payload.expires_at {
            return Err(CapabilityError::Expired {
                cap_id: payload.cap_id.clone(),
                expired_at: payload.expires_at,
                now,
            });
        }

        Ok(VerifiedCapability { payload })
    }

    /// Verify a token for execution, including scope and context checks.
    ///
    /// # Additional Checks (beyond `verify`)
    /// - Operation matches
    /// - Resource matches
    /// - All requested fields are authorized
    /// - Client binding (nonce/IP) matches if specified
    ///
    /// # Errors
    /// Returns appropriate `CapabilityError` for any validation failure.
    pub fn verify_for_execution(
        &self,
        token: &CapabilityToken,
        expected_op: &Opcode,
        expected_resource: &Resource,
        requested_fields: &FieldSet,
        client_nonce: Option<&str>,
        client_ip: Option<&str>,
        now: u64,
    ) -> Result<VerifiedCapability, CapabilityError> {
        // First, perform basic verification
        let verified = self.verify(token, now)?;

        // Check operation scope
        if verified.payload.op.as_str() != expected_op.as_str() {
            return Err(CapabilityError::ScopeMismatch {
                expected: verified.payload.op.as_str().to_string(),
                actual: expected_op.as_str().to_string(),
            });
        }

        // Check resource match
        if verified.payload.resource != *expected_resource {
            return Err(CapabilityError::ResourceMismatch {
                expected: verified.payload.resource.clone(),
                actual: expected_resource.clone(),
            });
        }

        // Check field authorization
        for field in &requested_fields.fields {
            if field != "*" && !verified.payload.fields.contains(field) {
                return Err(CapabilityError::FieldMismatch {
                    unauthorized_field: field.clone(),
                });
            }
        }

        // Check binding if present
        if let Some(ref bind) = verified.payload.bind {
            // Check nonce binding
            if let Some(expected_nonce) = &bind.client_nonce {
                match client_nonce {
                    Some(actual) if actual == expected_nonce => {}
                    Some(actual) => {
                        return Err(CapabilityError::BindingMismatch {
                            reason: format!(
                                "nonce mismatch: expected '{}', got '{}'",
                                expected_nonce, actual
                            ),
                        });
                    }
                    None => {
                        return Err(CapabilityError::BindingMismatch {
                            reason: "nonce required but not provided".to_string(),
                        });
                    }
                }
            }

            // Check IP binding
            if let Some(expected_ip) = &bind.ip_hint {
                match client_ip {
                    Some(actual) if actual == expected_ip => {}
                    Some(actual) => {
                        return Err(CapabilityError::BindingMismatch {
                            reason: format!(
                                "IP mismatch: expected '{}', got '{}'",
                                expected_ip, actual
                            ),
                        });
                    }
                    None => {
                        return Err(CapabilityError::BindingMismatch {
                            reason: "IP required but not provided".to_string(),
                        });
                    }
                }
            }
        }

        Ok(verified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilitySigner, SigningKey};
    use crate::protocol::{CapabilityBinding, opcode::*};

    fn setup_signer_verifier() -> (CapabilitySigner, CapabilityVerifier) {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();
        (
            CapabilitySigner::new(signing_key),
            CapabilityVerifier::new(verifying_key),
        )
    }

    fn create_test_payload(issued_at: u64, expires_at: u64) -> CapabilityPayload {
        CapabilityPayload::new(
            "cap-test-001",
            RESOURCE_READ,
            Resource::instance("user", "123"),
            FieldSet::new(["name", "email"]),
            issued_at,
            expires_at,
        )
    }

    // ==================== Basic Verification Tests ====================

    #[test]
    fn test_verify_valid_token() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;
        let payload = create_test_payload(1704067200, 1704067260);
        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify(&token, now);
        assert!(result.is_ok());

        let verified = result.unwrap();
        assert_eq!(verified.cap_id(), "cap-test-001");
        assert_eq!(verified.op().as_str(), RESOURCE_READ);
    }

    #[test]
    fn test_verify_expired_token() {
        let (signer, verifier) = setup_signer_verifier();
        let payload = create_test_payload(1704067200, 1704067260);
        let token = signer.mint(&payload).unwrap();

        // Verify at a time past expiry
        let now = 1704067300;
        let result = verifier.verify(&token, now);

        assert!(matches!(result, Err(CapabilityError::Expired { .. })));
    }

    #[test]
    fn test_verify_exactly_at_expiry_fails() {
        let (signer, verifier) = setup_signer_verifier();
        let expires_at = 1704067260;
        let payload = create_test_payload(1704067200, expires_at);
        let token = signer.mint(&payload).unwrap();

        // Verify exactly at expiry time
        let result = verifier.verify(&token, expires_at);
        assert!(matches!(result, Err(CapabilityError::Expired { .. })));
    }

    #[test]
    fn test_verify_clock_skew_rejected() {
        let (signer, verifier) = setup_signer_verifier();
        // Token issued far in the future
        let future_issued_at = 1704100000;
        let payload = create_test_payload(future_issued_at, future_issued_at + 60);
        let token = signer.mint(&payload).unwrap();

        // Verify at current time (way before issue time)
        let now = 1704067200;
        let result = verifier.verify(&token, now);

        assert!(matches!(result, Err(CapabilityError::ClockSkew { .. })));
    }

    #[test]
    fn test_verify_small_clock_skew_allowed() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067200;
        // Token issued 3 seconds in the future (within MAX_CLOCK_SKEW_SECS)
        let issued_at = now + 3;
        let payload = create_test_payload(issued_at, issued_at + 60);
        let token = signer.mint(&payload).unwrap();

        // Should pass (within tolerance)
        let result = verifier.verify(&token, now);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_invalid_signature_rejected() {
        let (signer, _) = setup_signer_verifier();
        // Create a different verifier with wrong key
        let wrong_verifier = CapabilityVerifier::new(SigningKey::generate().verifying_key());

        let payload = create_test_payload(1704067200, 1704067260);
        let token = signer.mint(&payload).unwrap();

        let result = wrong_verifier.verify(&token, 1704067230);
        assert!(matches!(result, Err(CapabilityError::InvalidSignature)));
    }

    #[test]
    fn test_verify_malformed_token_rejected() {
        let (_, verifier) = setup_signer_verifier();

        let bad_token = CapabilityToken::new("not-valid-base64!!!");
        let result = verifier.verify(&bad_token, 1704067230);

        assert!(matches!(result, Err(CapabilityError::TokenMalformed(_))));
    }

    #[test]
    fn test_verify_tampered_token_rejected() {
        let (signer, verifier) = setup_signer_verifier();
        let payload = create_test_payload(1704067200, 1704067260);
        let token = signer.mint(&payload).unwrap();

        // Tamper with the token
        let mut token_bytes = URL_SAFE_NO_PAD.decode(token.as_str()).unwrap();
        if !token_bytes.is_empty() {
            token_bytes[0] ^= 0xFF;
        }
        let tampered_token = CapabilityToken::new(URL_SAFE_NO_PAD.encode(&token_bytes));

        let result = verifier.verify(&tampered_token, 1704067230);
        // Should fail (either malformed or invalid signature)
        assert!(result.is_err());
    }

    // ==================== Execution Verification Tests ====================

    #[test]
    fn test_verify_for_execution_valid() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;
        let payload = create_test_payload(1704067200, 1704067260);
        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_READ),
            &Resource::instance("user", "123"),
            &FieldSet::new(["name"]),
            None,
            None,
            now,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_for_execution_scope_mismatch() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;
        let payload = create_test_payload(1704067200, 1704067260);
        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_DELETE), // Wrong operation
            &Resource::instance("user", "123"),
            &FieldSet::new(["name"]),
            None,
            None,
            now,
        );

        assert!(matches!(result, Err(CapabilityError::ScopeMismatch { .. })));
    }

    #[test]
    fn test_verify_for_execution_resource_type_mismatch() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;
        let payload = create_test_payload(1704067200, 1704067260);
        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_READ),
            &Resource::instance("document", "123"), // Wrong resource type
            &FieldSet::new(["name"]),
            None,
            None,
            now,
        );

        assert!(matches!(result, Err(CapabilityError::ResourceMismatch { .. })));
    }

    #[test]
    fn test_verify_for_execution_resource_id_mismatch() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;
        let payload = create_test_payload(1704067200, 1704067260);
        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_READ),
            &Resource::instance("user", "456"), // Wrong resource ID
            &FieldSet::new(["name"]),
            None,
            None,
            now,
        );

        assert!(matches!(result, Err(CapabilityError::ResourceMismatch { .. })));
    }

    #[test]
    fn test_verify_for_execution_field_mismatch() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;
        let payload = create_test_payload(1704067200, 1704067260);
        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_READ),
            &Resource::instance("user", "123"),
            &FieldSet::new(["password"]), // Unauthorized field
            None,
            None,
            now,
        );

        assert!(matches!(result, Err(CapabilityError::FieldMismatch { .. })));
    }

    #[test]
    fn test_verify_for_execution_with_wildcard_fields() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;

        // Payload with wildcard fields
        let payload = CapabilityPayload::new(
            "cap-all-fields",
            RESOURCE_READ,
            Resource::instance("user", "123"),
            FieldSet::all(),
            1704067200,
            1704067260,
        );
        let token = signer.mint(&payload).unwrap();

        // Should allow any field
        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_READ),
            &Resource::instance("user", "123"),
            &FieldSet::new(["any_field", "another_field"]),
            None,
            None,
            now,
        );

        assert!(result.is_ok());
    }

    // ==================== Binding Tests ====================

    #[test]
    fn test_verify_for_execution_nonce_binding_valid() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;

        let payload = CapabilityPayload::new(
            "cap-bound",
            RESOURCE_READ,
            Resource::instance("user", "123"),
            FieldSet::all(),
            1704067200,
            1704067260,
        )
        .with_binding(CapabilityBinding::with_nonce("nonce-xyz"));

        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_READ),
            &Resource::instance("user", "123"),
            &FieldSet::empty(),
            Some("nonce-xyz"), // Correct nonce
            None,
            now,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_for_execution_nonce_binding_mismatch() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;

        let payload = CapabilityPayload::new(
            "cap-bound",
            RESOURCE_READ,
            Resource::instance("user", "123"),
            FieldSet::all(),
            1704067200,
            1704067260,
        )
        .with_binding(CapabilityBinding::with_nonce("nonce-xyz"));

        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_READ),
            &Resource::instance("user", "123"),
            &FieldSet::empty(),
            Some("wrong-nonce"), // Wrong nonce
            None,
            now,
        );

        assert!(matches!(result, Err(CapabilityError::BindingMismatch { .. })));
    }

    #[test]
    fn test_verify_for_execution_nonce_binding_missing() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;

        let payload = CapabilityPayload::new(
            "cap-bound",
            RESOURCE_READ,
            Resource::instance("user", "123"),
            FieldSet::all(),
            1704067200,
            1704067260,
        )
        .with_binding(CapabilityBinding::with_nonce("nonce-xyz"));

        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_READ),
            &Resource::instance("user", "123"),
            &FieldSet::empty(),
            None, // Nonce required but not provided
            None,
            now,
        );

        assert!(matches!(result, Err(CapabilityError::BindingMismatch { .. })));
    }

    #[test]
    fn test_verify_for_execution_ip_binding_valid() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;

        let payload = CapabilityPayload::new(
            "cap-ip-bound",
            RESOURCE_READ,
            Resource::instance("user", "123"),
            FieldSet::all(),
            1704067200,
            1704067260,
        )
        .with_binding(CapabilityBinding::with_ip("192.168.1.100"));

        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_READ),
            &Resource::instance("user", "123"),
            &FieldSet::empty(),
            None,
            Some("192.168.1.100"), // Correct IP
            now,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_for_execution_ip_binding_mismatch() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;

        let payload = CapabilityPayload::new(
            "cap-ip-bound",
            RESOURCE_READ,
            Resource::instance("user", "123"),
            FieldSet::all(),
            1704067200,
            1704067260,
        )
        .with_binding(CapabilityBinding::with_ip("192.168.1.100"));

        let token = signer.mint(&payload).unwrap();

        let result = verifier.verify_for_execution(
            &token,
            &Opcode::new(RESOURCE_READ),
            &Resource::instance("user", "123"),
            &FieldSet::empty(),
            None,
            Some("10.0.0.1"), // Wrong IP
            now,
        );

        assert!(matches!(result, Err(CapabilityError::BindingMismatch { .. })));
    }

    // ==================== VerifiedCapability Accessor Tests ====================

    #[test]
    fn test_verified_capability_accessors() {
        let (signer, verifier) = setup_signer_verifier();
        let now = 1704067230;
        let payload = create_test_payload(1704067200, 1704067260);
        let token = signer.mint(&payload).unwrap();

        let verified = verifier.verify(&token, now).unwrap();

        assert_eq!(verified.cap_id(), "cap-test-001");
        assert_eq!(verified.op().as_str(), RESOURCE_READ);
        assert_eq!(verified.resource().resource_type, "user");
        assert_eq!(verified.resource().resource_id.as_deref(), Some("123"));
        assert!(verified.fields().contains("name"));
        assert!(verified.fields().contains("email"));
        assert_eq!(verified.issued_at(), 1704067200);
        assert_eq!(verified.expires_at(), 1704067260);
    }
}
