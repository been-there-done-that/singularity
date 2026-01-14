//! JWT verifier implementation.
//!
//! Supports HS256, RS256, and ES256 algorithms.

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use std::collections::HashMap;

use crate::policy::PolicySubject;

use super::claims::StandardClaims;
use super::error::IdentityError;
use super::traits::IdentityVerifier;

/// Key source for JWT verification.
///
/// Explicit separation of symmetric vs asymmetric trust models.
#[derive(Clone)]
pub enum JwtKeySource {
    /// HMAC shared secret (HS256).
    HmacSecret(Vec<u8>),
    /// RSA public keys keyed by kid (RS256).
    RsaPublicKeys(HashMap<String, DecodingKey>),
    /// EC public keys keyed by kid (ES256).
    EcPublicKeys(HashMap<String, DecodingKey>),
    /// Single RSA public key (no kid selection).
    RsaSingleKey(DecodingKey),
    /// Single EC public key (no kid selection).
    EcSingleKey(DecodingKey),
}

/// JWT verifier configuration.
#[derive(Clone)]
pub struct JwtVerifier {
    /// Expected issuer (iss claim).
    issuer: String,
    /// Expected audience (aud claim).
    audience: String,
    /// Key source for verification.
    keys: JwtKeySource,
    /// Clock skew tolerance in seconds.
    clock_skew_secs: u64,
}

impl JwtVerifier {
    /// Create a new JWT verifier with HMAC secret.
    pub fn with_hmac_secret(
        issuer: impl Into<String>,
        audience: impl Into<String>,
        secret: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            issuer: issuer.into(),
            audience: audience.into(),
            keys: JwtKeySource::HmacSecret(secret.into()),
            clock_skew_secs: 60,
        }
    }

    /// Create a new JWT verifier with RSA public key (PEM format).
    pub fn with_rsa_pem(
        issuer: impl Into<String>,
        audience: impl Into<String>,
        pem: &[u8],
    ) -> Result<Self, IdentityError> {
        let key = DecodingKey::from_rsa_pem(pem)
            .map_err(|e| IdentityError::InvalidFormat(e.to_string()))?;

        Ok(Self {
            issuer: issuer.into(),
            audience: audience.into(),
            keys: JwtKeySource::RsaSingleKey(key),
            clock_skew_secs: 60,
        })
    }

    /// Create a new JWT verifier with EC public key (PEM format).
    pub fn with_ec_pem(
        issuer: impl Into<String>,
        audience: impl Into<String>,
        pem: &[u8],
    ) -> Result<Self, IdentityError> {
        let key = DecodingKey::from_ec_pem(pem)
            .map_err(|e| IdentityError::InvalidFormat(e.to_string()))?;

        Ok(Self {
            issuer: issuer.into(),
            audience: audience.into(),
            keys: JwtKeySource::EcSingleKey(key),
            clock_skew_secs: 60,
        })
    }

    /// Set clock skew tolerance.
    pub fn with_clock_skew(mut self, secs: u64) -> Self {
        self.clock_skew_secs = secs;
        self
    }

    /// Get the algorithm for this key source.
    fn algorithm(&self) -> Algorithm {
        match &self.keys {
            JwtKeySource::HmacSecret(_) => Algorithm::HS256,
            JwtKeySource::RsaPublicKeys(_) | JwtKeySource::RsaSingleKey(_) => Algorithm::RS256,
            JwtKeySource::EcPublicKeys(_) | JwtKeySource::EcSingleKey(_) => Algorithm::ES256,
        }
    }

    /// Get the decoding key for verification.
    fn get_key(&self, kid: Option<&str>) -> Result<DecodingKey, IdentityError> {
        match &self.keys {
            JwtKeySource::HmacSecret(secret) => {
                Ok(DecodingKey::from_secret(secret))
            }
            JwtKeySource::RsaSingleKey(key) | JwtKeySource::EcSingleKey(key) => {
                Ok(key.clone())
            }
            JwtKeySource::RsaPublicKeys(keys) | JwtKeySource::EcPublicKeys(keys) => {
                let kid = kid.ok_or_else(|| {
                    IdentityError::InvalidFormat("kid header required for key selection".to_string())
                })?;
                keys.get(kid)
                    .cloned()
                    .ok_or_else(|| IdentityError::KeyNotFound(kid.to_string()))
            }
        }
    }
}

impl IdentityVerifier for JwtVerifier {
    fn verify(&self, token: &str, now: u64) -> Result<PolicySubject, IdentityError> {
        // Extract header to get kid if present
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| IdentityError::InvalidFormat(e.to_string()))?;

        // Get the appropriate key
        let key = self.get_key(header.kid.as_deref())?;

        // Build validation - disable time validation (we do it manually with explicit now)
        let mut validation = Validation::new(self.algorithm());
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.validate_exp = false;  // We validate manually
        validation.validate_nbf = false;  // We validate manually

        // Decode and validate
        let token_data = decode::<StandardClaims>(token, &key, &validation)
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("InvalidSignature") {
                    IdentityError::InvalidSignature
                } else if err_str.contains("InvalidIssuer") {
                    IdentityError::IssuerMismatch {
                        expected: self.issuer.clone(),
                        actual: "unknown".to_string(),
                    }
                } else if err_str.contains("InvalidAudience") {
                    IdentityError::AudienceMismatch
                } else {
                    IdentityError::InvalidFormat(err_str)
                }
            })?;


        let claims = token_data.claims;

        // Additional time validation with explicit now
        if let Some(exp) = claims.exp {
            if now > exp + self.clock_skew_secs {
                return Err(IdentityError::TokenExpired);
            }
        }

        if let Some(nbf) = claims.nbf {
            if now + self.clock_skew_secs < nbf {
                return Err(IdentityError::TokenNotYetValid);
            }
        }

        // Convert to PolicySubject
        Ok(claims.into_policy_subject())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn create_test_token(claims: &StandardClaims, secret: &[u8]) -> String {
        let header = Header::new(Algorithm::HS256);
        encode(&header, claims, &EncodingKey::from_secret(secret)).unwrap()
    }

    #[test]
    fn test_jwt_verifier_valid_token() {
        let secret = b"test-secret-key-for-hmac-256-algorithm";
        let verifier = JwtVerifier::with_hmac_secret(
            "https://auth.example.com",
            "singularity",
            secret.to_vec(),
        );

        let claims = StandardClaims {
            sub: "user-123".to_string(),
            roles: vec!["admin".to_string()],
            groups: vec![],
            email: Some("user@example.com".to_string()),
            name: None,
            iat: Some(1704067200),
            exp: Some(1704070800),
            nbf: Some(1704067200),
            iss: Some("https://auth.example.com".to_string()),
            aud: Some(serde_json::json!("singularity")),
        };

        let token = create_test_token(&claims, secret);
        let now = 1704067500; // Within validity window

        let result = verifier.verify(&token, now);
        assert!(result.is_ok());

        let subject = result.unwrap();
        assert_eq!(subject.id, "user-123");
        assert!(subject.has_role("admin"));
    }

    #[test]
    fn test_jwt_verifier_expired_token() {
        let secret = b"test-secret-key-for-hmac-256-algorithm";
        let verifier = JwtVerifier::with_hmac_secret(
            "https://auth.example.com",
            "singularity",
            secret.to_vec(),
        );

        let claims = StandardClaims {
            sub: "user-123".to_string(),
            roles: vec![],
            groups: vec![],
            email: None,
            name: None,
            iat: Some(1704067200),
            exp: Some(1704067260), // Expires quickly
            nbf: None,
            iss: Some("https://auth.example.com".to_string()),
            aud: Some(serde_json::json!("singularity")),
        };

        let token = create_test_token(&claims, secret);
        let now = 1704070800; // Way past expiration

        let result = verifier.verify(&token, now);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_expired());
    }

    #[test]
    fn test_jwt_verifier_invalid_signature() {
        let secret1 = b"correct-secret-key-for-signing!";
        let secret2 = b"wrong-secret-key-for-verifying";

        let verifier = JwtVerifier::with_hmac_secret(
            "https://auth.example.com",
            "singularity",
            secret2.to_vec(),
        );

        let claims = StandardClaims {
            sub: "user-123".to_string(),
            roles: vec![],
            groups: vec![],
            email: None,
            name: None,
            iat: Some(1704067200),
            exp: Some(1704070800),
            nbf: None,
            iss: Some("https://auth.example.com".to_string()),
            aud: Some(serde_json::json!("singularity")),
        };

        let token = create_test_token(&claims, secret1);
        let now = 1704067500;

        let result = verifier.verify(&token, now);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_signature_error());
    }

    #[test]
    fn test_jwt_verifier_clock_skew() {
        let secret = b"test-secret-key-for-hmac-256-algorithm";
        let verifier = JwtVerifier::with_hmac_secret(
            "https://auth.example.com",
            "singularity",
            secret.to_vec(),
        ).with_clock_skew(120); // 2 minutes tolerance

        let claims = StandardClaims {
            sub: "user-123".to_string(),
            roles: vec![],
            groups: vec![],
            email: None,
            name: None,
            iat: Some(1704067200),
            exp: Some(1704067260),
            nbf: None,
            iss: Some("https://auth.example.com".to_string()),
            aud: Some(serde_json::json!("singularity")),
        };

        let token = create_test_token(&claims, secret);
        let now = 1704067320; // 1 minute past expiry, within 2 min skew

        let result = verifier.verify(&token, now);
        assert!(result.is_ok());
    }
}
