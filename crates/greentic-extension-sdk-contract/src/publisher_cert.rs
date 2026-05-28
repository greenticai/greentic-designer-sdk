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
    /// # Errors
    /// `CertInvalid` on any decode/length/signature failure.
    ///
    /// Note: `not_after` is not enforced here; expiry is the caller's responsibility.
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
}
