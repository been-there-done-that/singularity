//! JWT issuance for authenticated users.
//!
//! # Token Format
//!
//! Uses HS256 (HMAC-SHA256) for simplicity in single-node deployment.
//! Production should use EdDSA (Ed25519) with key rotation.

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JwtIssueError {
    #[error("encoding failed: {0}")]
    EncodingFailed(String),
}

/// Claims included in issued JWTs.
/// Matches the format expected by JwtVerifier.
#[derive(Debug, Serialize, Deserialize)]
pub struct IssuedClaims {
    /// Subject (user ID)
    pub sub: String,
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: String,
    /// Issued at (Unix timestamp)
    pub iat: u64,
    /// Expiration (Unix timestamp)
    pub exp: u64,
    /// User roles
    pub roles: Vec<String>,
    /// Email (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// JWT issuer configuration.
pub struct JwtIssuer {
    /// Secret for HMAC signing
    secret: Vec<u8>,
    /// Issuer claim value
    issuer: String,
    /// Audience claim value
    audience: String,
    /// Token TTL in seconds (default: 30 min)
    ttl_secs: u64,
}

impl JwtIssuer {
    /// Create a new JWT issuer.
    pub fn new(
        secret: impl Into<Vec<u8>>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> Self {
        Self {
            secret: secret.into(),
            issuer: issuer.into(),
            audience: audience.into(),
            ttl_secs: 30 * 60, // 30 minutes
        }
    }

    /// Set custom TTL.
    pub fn with_ttl(mut self, secs: u64) -> Self {
        self.ttl_secs = secs;
        self
    }

    /// Issue a JWT for an authenticated user.
    pub fn issue(
        &self,
        user_id: &str,
        roles: Vec<String>,
        email: Option<String>,
    ) -> Result<String, JwtIssueError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let claims = IssuedClaims {
            sub: user_id.to_string(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            iat: now,
            exp: now + self.ttl_secs,
            roles,
            email,
        };

        let header = Header::new(Algorithm::HS256);
        let key = EncodingKey::from_secret(&self.secret);

        encode(&header, &claims, &key).map_err(|e| JwtIssueError::EncodingFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, DecodingKey, Validation};

    #[test]
    fn test_issue_and_decode() {
        let secret = b"test-secret-key-for-testing-only";
        let issuer = JwtIssuer::new(secret.to_vec(), "https://singularity.local", "singularity");

        let token = issuer
            .issue("user-123", vec!["admin".to_string()], Some("user@example.com".to_string()))
            .unwrap();

        // Decode and verify
        let key = DecodingKey::from_secret(secret);
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&["https://singularity.local"]);
        validation.set_audience(&["singularity"]);

        let decoded = decode::<IssuedClaims>(&token, &key, &validation).unwrap();

        assert_eq!(decoded.claims.sub, "user-123");
        assert_eq!(decoded.claims.roles, vec!["admin"]);
        assert_eq!(decoded.claims.email, Some("user@example.com".to_string()));
    }
}
