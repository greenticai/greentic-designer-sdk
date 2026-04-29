use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::describe::DescribeJson;
use crate::error::ContractError;

/// Compute SHA256 of artifact bytes as hex string.
#[must_use]
pub fn artifact_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[must_use]
pub fn sign_ed25519(key: &SigningKey, payload: &[u8]) -> String {
    use ed25519_dalek::Signer;
    let sig: Signature = key.sign(payload);
    B64.encode(sig.to_bytes())
}

pub fn verify_ed25519(
    public_key_b64: &str,
    signature_b64: &str,
    payload: &[u8],
) -> Result<(), ContractError> {
    let public_key_bytes = B64
        .decode(strip_prefix(public_key_b64))
        .map_err(|e| ContractError::SignatureInvalid(format!("pubkey b64: {e}")))?;
    let sig_bytes = B64
        .decode(signature_b64)
        .map_err(|e| ContractError::SignatureInvalid(format!("sig b64: {e}")))?;
    let public_key_array: [u8; 32] = public_key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ContractError::SignatureInvalid("pubkey length != 32".into()))?;
    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ContractError::SignatureInvalid("sig length != 64".into()))?;
    let key = VerifyingKey::from_bytes(&public_key_array)
        .map_err(|e| ContractError::SignatureInvalid(format!("pubkey parse: {e}")))?;
    let signature = Signature::from_bytes(&sig_array);
    key.verify(payload, &signature)
        .map_err(|e| ContractError::SignatureInvalid(format!("verify: {e}")))
}

/// Canonicalize describe.json for signing — strip the `.signature` field
/// and emit RFC 8785 JCS bytes. Output is deterministic across languages
/// and serde versions.
pub fn canonical_signing_payload(describe: &DescribeJson) -> Result<Vec<u8>, ContractError> {
    let mut clone = describe.clone();
    clone.signature = None;
    serde_jcs::to_vec(&clone).map_err(|e| ContractError::Canonicalize(e.to_string()))
}

/// Sign describe.json in-place. Strips any existing `.signature` field,
/// canonicalizes via JCS, signs the canonical bytes, and injects a fresh
/// `.signature` object. Safe to call on already-signed describe (produces
/// identical bytes regardless of prior sig).
pub fn sign_describe(
    describe: &mut DescribeJson,
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<(), ContractError> {
    use ed25519_dalek::Signer;
    // Defensive: strip before canonicalize so the sig is computed on clean payload.
    describe.signature = None;
    let payload = canonical_signing_payload(describe)?;
    let sig = signing_key.sign(&payload);
    let pubkey_b64 = B64.encode(signing_key.verifying_key().to_bytes());
    let sig_b64 = B64.encode(sig.to_bytes());
    describe.signature = Some(crate::describe::Signature {
        algorithm: "ed25519".into(),
        public_key: pubkey_b64,
        value: sig_b64,
    });
    Ok(())
}

/// Verify the inline `.signature` field of a describe.json. Returns
/// `Ok(())` iff signature is present, algorithm is `ed25519`, and the
/// signature matches the canonical payload (describe with `.signature`
/// stripped, serialized via JCS).
pub fn verify_describe(describe: &DescribeJson) -> Result<(), ContractError> {
    let sig = describe
        .signature
        .as_ref()
        .ok_or_else(|| ContractError::SignatureInvalid("missing signature field".into()))?;
    if sig.algorithm != "ed25519" {
        return Err(ContractError::SignatureInvalid(format!(
            "unsupported algorithm: {}",
            sig.algorithm
        )));
    }
    let payload = canonical_signing_payload(describe)?;
    verify_ed25519(&sig.public_key, &sig.value, &payload)
}

fn strip_prefix(s: &str) -> &str {
    s.strip_prefix("ed25519:").unwrap_or(s)
}
