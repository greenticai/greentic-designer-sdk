use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::describe::DescribeJson;
use crate::error::ContractError;

fn decode_ed25519_pubkey(s: &str) -> Result<ed25519_dalek::VerifyingKey, ContractError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(strip_prefix(s))
        .map_err(|e| ContractError::SignatureInvalid(format!("pubkey b64: {e}")))?;
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| ContractError::SignatureInvalid("pubkey length != 32".into()))?;
    ed25519_dalek::VerifyingKey::from_bytes(&arr)
        .map_err(|e| ContractError::SignatureInvalid(format!("pubkey parse: {e}")))
}

/// Compute SHA256 of artifact bytes as hex string.
#[must_use]
pub fn artifact_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    crate::hex::encode(hasher.finalize().as_slice())
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
    key.verify_strict(payload, &signature)
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
        algorithm: crate::describe::SignatureAlgorithm::Ed25519,
        public_key: pubkey_b64,
        value: sig_b64,
        key_id: None,
    });
    Ok(())
}

/// Integrity-only check: verifies the inline signature against the key the
/// describe *asserts about itself*. This proves the describe has not changed
/// since signing, but NOT who signed it — an attacker can re-sign with their
/// own key. Callers needing authenticity MUST use [`verify_describe_with_key`]
/// against a trust-anchored key (audit C1).
pub fn verify_describe_self_consistent(describe: &DescribeJson) -> Result<(), ContractError> {
    let sig = describe
        .signature
        .as_ref()
        .ok_or_else(|| ContractError::SignatureInvalid("missing signature field".into()))?;
    if !matches!(sig.algorithm, crate::describe::SignatureAlgorithm::Ed25519) {
        return Err(ContractError::SignatureInvalid(
            "unsupported algorithm".into(),
        ));
    }
    let payload = canonical_signing_payload(describe)?;
    verify_ed25519(&sig.public_key, &sig.value, &payload)
}

/// Authenticity check: verifies the inline signature was produced by
/// `trusted_key` AND that the describe's self-asserted key matches it. This is
/// the C1-correct verification — the key must come from a trust anchor
/// (trust store, TOFU pin, or a `PublisherCert` resolved via `RootVerifier`),
/// never from the artifact alone.
///
/// # Errors
/// `SignatureInvalid` if the signature field is missing, the algorithm is not
/// ed25519, the self-asserted key does not match `trusted_key`, or the
/// signature does not verify.
pub fn verify_describe_with_key(
    describe: &DescribeJson,
    trusted_key: &ed25519_dalek::VerifyingKey,
) -> Result<(), ContractError> {
    let sig = describe
        .signature
        .as_ref()
        .ok_or_else(|| ContractError::SignatureInvalid("missing signature field".into()))?;
    if !matches!(sig.algorithm, crate::describe::SignatureAlgorithm::Ed25519) {
        return Err(ContractError::SignatureInvalid(
            "unsupported algorithm".into(),
        ));
    }
    let asserted = decode_ed25519_pubkey(&sig.public_key)?;
    if asserted.to_bytes() != trusted_key.to_bytes() {
        return Err(ContractError::SignatureInvalid(
            "describe signing key does not match the trusted key".into(),
        ));
    }
    let payload = canonical_signing_payload(describe)?;
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(&sig.value)
        .map_err(|e| ContractError::SignatureInvalid(format!("sig b64: {e}")))?;
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ContractError::SignatureInvalid("sig length != 64".into()))?;
    trusted_key
        .verify_strict(&payload, &ed25519_dalek::Signature::from_bytes(&sig_arr))
        .map_err(|e| ContractError::SignatureInvalid(format!("verify: {e}")))
}

/// Compute `sha256(manifest_bytes)` and store it in `describe.manifest_sha256`
/// so the describe signature transitively covers the whole-archive manifest.
///
/// # Ordering requirement
/// MUST be called **before** [`sign_describe`]. `sign_describe` canonicalizes
/// `manifest_sha256` into the JCS signing payload; calling `bind_manifest` after
/// signing leaves the field out of the signed payload and the manifest is NOT
/// covered by the signature (an insecure artifact).
pub fn bind_manifest(describe: &mut DescribeJson, manifest_bytes: &[u8]) {
    describe.manifest_sha256 = Some(artifact_sha256(manifest_bytes));
}

/// Verify the describe's `manifest_sha256` matches the supplied manifest bytes.
///
/// This checks the manifest binding only; verify the describe signature
/// separately (see the `verify_describe`* functions).
///
/// # Errors
/// `SignatureInvalid` if the field is absent or does not match.
pub fn verify_manifest_binding(
    describe: &DescribeJson,
    manifest_bytes: &[u8],
) -> Result<(), ContractError> {
    let expected = describe
        .manifest_sha256
        .as_deref()
        .ok_or_else(|| ContractError::SignatureInvalid("describe.manifestSha256 missing".into()))?;
    let actual = artifact_sha256(manifest_bytes);
    if expected != actual {
        return Err(ContractError::SignatureInvalid(format!(
            "manifestSha256 mismatch: stored={expected}, computed={actual}"
        )));
    }
    Ok(())
}

fn strip_prefix(s: &str) -> &str {
    s.strip_prefix("ed25519:").unwrap_or(s)
}
