//! Trust anchor for publisher certs (audit C1 machinery).
//!
//! Production verification chains a [`PublisherCert`] to a Greentic root key.
//! The production root public key is **not yet provisioned** — that decision
//! (KMS custody, HSM vs KMS, rotation/DR) is org-blocked. Until then,
//! [`EmbeddedRootVerifier::from_embedded`] returns
//! [`ContractError::TrustRootUnavailable`]. The Strict-via-trust-store path
//! (registry crate) and the Normal/TOFU path do not require this root, so the
//! machinery is fully testable today via [`FixtureRootVerifier`].

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::VerifyingKey;

use crate::error::ContractError;
use crate::publisher_cert::PublisherCert;

/// Resolves an authorized publisher key from a [`PublisherCert`] by checking
/// the cert chains to a trusted root.
pub trait RootVerifier {
    /// Verify `cert` against the trusted root, returning the authorized
    /// publisher key.
    ///
    /// # Errors
    /// `CertInvalid` if the chain does not verify; `TrustRootUnavailable` if
    /// no root is configured.
    fn verify_cert(&self, cert: &PublisherCert) -> Result<VerifyingKey, ContractError>;
}

/// Base64-encoded production Greentic root public key.
///
/// TODO(org): provision the production root key (KMS) and paste its 32-byte
/// ed25519 public key here (base64). Until then the embedded verifier is
/// unavailable and Strict-via-cert installs are blocked — see spec residual.
const PROD_ROOT_PUBKEY_B64: &str = "";

/// Verifier backed by the compiled-in production root key.
#[derive(Debug, Clone)]
pub struct EmbeddedRootVerifier {
    root: VerifyingKey,
}

impl EmbeddedRootVerifier {
    /// Construct from the embedded production root key.
    ///
    /// # Errors
    /// `TrustRootUnavailable` while the production key is unprovisioned
    /// (org-blocked), or `CertInvalid` if the embedded value is malformed.
    pub fn from_embedded() -> Result<Self, ContractError> {
        if PROD_ROOT_PUBKEY_B64.is_empty() {
            return Err(ContractError::TrustRootUnavailable(
                "production root key not yet provisioned (org-blocked)".into(),
            ));
        }
        let bytes = B64
            .decode(PROD_ROOT_PUBKEY_B64)
            .map_err(|e| ContractError::CertInvalid(format!("embedded root b64: {e}")))?;
        let arr: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| ContractError::CertInvalid("embedded root length != 32".into()))?;
        let root = VerifyingKey::from_bytes(&arr)
            .map_err(|e| ContractError::CertInvalid(format!("embedded root parse: {e}")))?;
        Ok(Self { root })
    }
}

impl RootVerifier for EmbeddedRootVerifier {
    fn verify_cert(&self, cert: &PublisherCert) -> Result<VerifyingKey, ContractError> {
        cert.verify(&self.root)
    }
}

/// Test-only verifier with a caller-supplied root key.
#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Clone)]
pub struct FixtureRootVerifier {
    root: VerifyingKey,
}

#[cfg(any(test, feature = "testing"))]
impl FixtureRootVerifier {
    /// Construct with the given root key (test/integration use only).
    #[must_use]
    pub fn new(root: VerifyingKey) -> Self {
        Self { root }
    }
}

#[cfg(any(test, feature = "testing"))]
impl RootVerifier for FixtureRootVerifier {
    fn verify_cert(&self, cert: &PublisherCert) -> Result<VerifyingKey, ContractError> {
        cert.verify(&self.root)
    }
}
