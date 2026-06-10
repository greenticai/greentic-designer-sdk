//! `PublisherCert` — a Greentic-root-signed attestation binding a publisher's
//! ed25519 public key. The root signs the publisher's 32-byte public key;
//! verification recovers the authorized publisher key (audit C1 machinery).

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::ContractError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublisherCert {
    /// Base64 of the publisher's 32-byte ed25519 public key.
    #[serde(rename = "publisherPublicKey")]
    pub publisher_public_key: String,
    /// Base64 of the root's 64-byte ed25519 signature over the publisher key.
    #[serde(rename = "rootSignature")]
    pub root_signature: String,
    /// Opaque identifier for the root key that issued this cert — informational only.
    #[serde(rename = "keyId", default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    /// Optional RFC3339 expiry. Enforcement is the caller's responsibility.
    #[serde(rename = "notAfter", default, skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
}

impl PublisherCert {
    /// Verify this cert was signed by `root`. Returns the authorized publisher
    /// verifying key on success.
    ///
    /// # Note
    /// `not_after` is not enforced here; expiry is the caller's responsibility.
    ///
    /// # Errors
    /// `CertInvalid` on any decode/length/signature failure.
    pub fn verify(&self, root: &VerifyingKey) -> Result<VerifyingKey, ContractError> {
        let pub_bytes = B64
            .decode(&self.publisher_public_key)
            .map_err(|e| ContractError::CertInvalid(format!("publisher key b64: {e}")))?;
        let pub_arr: [u8; 32] = pub_bytes
            .as_slice()
            .try_into()
            .map_err(|_| ContractError::CertInvalid("publisher key length != 32".into()))?;
        let sig_bytes = B64
            .decode(&self.root_signature)
            .map_err(|e| ContractError::CertInvalid(format!("root sig b64: {e}")))?;
        let sig_arr: [u8; 64] = sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| ContractError::CertInvalid("root sig length != 64".into()))?;
        let signature = Signature::from_bytes(&sig_arr);
        let publisher_key = VerifyingKey::from_bytes(&pub_arr)
            .map_err(|e| ContractError::CertInvalid(format!("publisher key parse: {e}")))?;
        root.verify_strict(publisher_key.as_bytes(), &signature)
            .map_err(|e| ContractError::CertInvalid(format!("root signature: {e}")))?;
        Ok(publisher_key)
    }

    /// Whether this cert is expired at `now` per its `notAfter` field.
    ///
    /// Callers enforcing expiry should reject when this returns `Ok(true)`.
    /// A cert without `notAfter` never expires (`Ok(false)`).
    ///
    /// # Errors
    /// `CertInvalid` if `notAfter` is present but not valid RFC3339 — a
    /// malformed expiry must fail closed, not be treated as "no expiry".
    pub fn is_expired(&self, now: chrono::DateTime<chrono::Utc>) -> Result<bool, ContractError> {
        let Some(not_after) = self.not_after.as_deref() else {
            return Ok(false);
        };
        let expiry = chrono::DateTime::parse_from_rfc3339(not_after)
            .map_err(|e| ContractError::CertInvalid(format!("notAfter: {e}")))?;
        Ok(now > expiry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cert_with_not_after(not_after: Option<&str>) -> PublisherCert {
        PublisherCert {
            publisher_public_key: String::new(),
            root_signature: String::new(),
            key_id: None,
            not_after: not_after.map(str::to_string),
        }
    }

    fn at(rfc3339: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn cert_without_not_after_never_expires() {
        let cert = cert_with_not_after(None);
        assert!(!cert.is_expired(at("2099-01-01T00:00:00Z")).unwrap());
    }

    #[test]
    fn cert_is_not_expired_before_not_after() {
        let cert = cert_with_not_after(Some("2030-01-01T00:00:00Z"));
        assert!(!cert.is_expired(at("2026-06-10T00:00:00Z")).unwrap());
    }

    #[test]
    fn cert_is_expired_after_not_after() {
        let cert = cert_with_not_after(Some("2020-01-01T00:00:00Z"));
        assert!(cert.is_expired(at("2026-06-10T00:00:00Z")).unwrap());
    }

    #[test]
    fn cert_expiry_boundary_is_inclusive() {
        // Exactly at notAfter the cert is still valid (`now > expiry` only).
        let cert = cert_with_not_after(Some("2026-06-10T00:00:00Z"));
        assert!(!cert.is_expired(at("2026-06-10T00:00:00Z")).unwrap());
    }

    #[test]
    fn malformed_not_after_fails_closed() {
        let cert = cert_with_not_after(Some("not-a-date"));
        assert!(matches!(
            cert.is_expired(at("2026-06-10T00:00:00Z")),
            Err(ContractError::CertInvalid(_))
        ));
    }
}
