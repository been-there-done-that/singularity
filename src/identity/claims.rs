//! JWT claim mapping to PolicySubject.
//!
//! # Invariant
//!
//! > **Only explicitly mapped claims enter PolicySubject.**
//!
//! We do NOT dump all JWT claims blindly.

use serde::Deserialize;

use crate::policy::PolicySubject;

/// Standard JWT claims we extract.
///
/// Only these claims are trusted. Unmapped claims are ignored.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct StandardClaims {
    /// Subject identifier (required).
    pub sub: String,

    /// Session ID (for revocation).
    #[serde(default)]
    pub sid: Option<String>,

    /// Session key hash (proof of possession).
    #[serde(default)]
    pub skh: Option<String>,

    /// User roles (optional, defaults to empty).
    #[serde(default)]
    pub roles: Vec<String>,

    /// Alternative: groups claim (some IdPs use this).
    #[serde(default)]
    pub groups: Vec<String>,

    /// Email (optional, mapped to claims).
    #[serde(default)]
    pub email: Option<String>,

    /// Name (optional, mapped to claims).
    #[serde(default)]
    pub name: Option<String>,

    /// Issued at timestamp.
    #[serde(default)]
    pub iat: Option<u64>,

    /// Expiration timestamp.
    #[serde(default)]
    pub exp: Option<u64>,

    /// Not before timestamp.
    #[serde(default)]
    pub nbf: Option<u64>,

    /// Issuer.
    #[serde(default)]
    pub iss: Option<String>,

    /// Audience.
    #[serde(default)]
    pub aud: Option<serde_json::Value>,
}

impl StandardClaims {
    /// Convert claims to PolicySubject.
    ///
    /// # Rules
    ///
    /// - `sub` → `id` (required)
    /// - `roles` OR `groups` → `roles` (merged)
    /// - `email`, `name` → `claims` map
    pub fn into_policy_subject(self) -> PolicySubject {
        // Merge roles and groups
        let mut roles = self.roles;
        roles.extend(self.groups);

        // Build subject
        let mut subject = PolicySubject::new(self.sub).with_roles(roles);

        // Add optional claims
        if let Some(email) = self.email {
            subject = subject.with_claim("email", serde_json::json!(email));
        }
        if let Some(name) = self.name {
            subject = subject.with_claim("name", serde_json::json!(name));
        }
        // Add session claims for verification
        if let Some(sid) = self.sid {
            subject = subject.with_claim("sid", serde_json::json!(sid));
        }
        if let Some(skh) = self.skh {
            subject = subject.with_claim("skh", serde_json::json!(skh));
        }

        subject
    }

    /// Get session ID if present.
    pub fn session_id(&self) -> Option<&str> {
        self.sid.as_deref()
    }

    /// Get session key hash if present.
    pub fn session_key_hash(&self) -> Option<&str> {
        self.skh.as_deref()
    }
}

/// Claim extractor configuration.
#[derive(Debug, Clone)]
pub struct ClaimConfig {
    /// Field name for subject ID (default: "sub").
    pub subject_claim: String,
    /// Field name for roles (default: "roles").
    pub roles_claim: String,
    /// Alternative field name for roles (default: "groups").
    pub groups_claim: String,
}

impl Default for ClaimConfig {
    fn default() -> Self {
        Self {
            subject_claim: "sub".to_string(),
            roles_claim: "roles".to_string(),
            groups_claim: "groups".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claims_to_policy_subject() {
        let claims = StandardClaims {
            sub: "user-123".to_string(),
            sid: Some("session-1".to_string()),
            skh: Some("hash-1".to_string()),
            roles: vec!["admin".to_string()],
            groups: vec!["engineering".to_string()],
            email: Some("user@example.com".to_string()),
            name: Some("Test User".to_string()),
            iat: None,
            exp: None,
            nbf: None,
            iss: None,
            aud: None,
        };

        let subject = claims.into_policy_subject();
        assert_eq!(subject.id, "user-123");
        assert!(subject.has_role("admin"));
        assert!(subject.has_role("engineering"));
        assert_eq!(
            subject.claims.get("email"),
            Some(&serde_json::json!("user@example.com"))
        );
    }

    #[test]
    fn test_claims_missing_roles_ok() {
        let claims = StandardClaims {
            sub: "user-456".to_string(),
            sid: None,
            skh: None,
            roles: vec![],
            groups: vec![],
            email: None,
            name: None,
            iat: None,
            exp: None,
            nbf: None,
            iss: None,
            aud: None,
        };

        let subject = claims.into_policy_subject();
        assert_eq!(subject.id, "user-456");
        assert!(subject.roles.is_empty());
    }
}
