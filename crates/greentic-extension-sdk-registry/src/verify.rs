//! Pre-install verification: whole-archive integrity (manifest ledger,
//! describe binding) and publisher authenticity (signature + trust anchor).
//! Split out of `lifecycle.rs` so trust decisions can be reviewed and tested
//! independently of install orchestration.

use crate::error::RegistryError;
use crate::lifecycle::TrustPolicy;
use crate::types::ExtensionArtifact;

/// Parse a base64 (optionally `ed25519:`-prefixed) ed25519 public key.
fn parse_verifying_key(b64: &str) -> Result<ed25519_dalek::VerifyingKey, RegistryError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let bytes = B64
        .decode(b64.strip_prefix("ed25519:").unwrap_or(b64))
        .map_err(|e| RegistryError::SignatureInvalid(format!("publisher key b64: {e}")))?;
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| RegistryError::SignatureInvalid("publisher key length != 32".into()))?;
    ed25519_dalek::VerifyingKey::from_bytes(&arr)
        .map_err(|e| RegistryError::SignatureInvalid(format!("publisher key parse: {e}")))
}

/// Verify the describe is authentically signed *by a trusted key* — not merely
/// self-consistent. The signature is checked first (so a bad signature never
/// poisons the trust store), then the trust anchor is applied per policy:
/// - `Loose`  → skipped (dev bypass).
/// - `Normal` → TOFU: pin the signing key on first install, require it after.
/// - `Strict` → the signing key must already be in the trust store.
///
/// (`PublisherCert`→root resolution is wired in the contract crate but stays
/// fail-closed until the production root key is provisioned, so Strict today
/// trusts keys pinned via a prior Normal install or added out-of-band.)
pub(crate) fn verify_authenticity(
    root: &std::path::Path,
    describe: &greentic_extension_sdk_contract::DescribeJson,
    policy: TrustPolicy,
) -> Result<(), RegistryError> {
    if policy == TrustPolicy::Loose {
        return Ok(());
    }
    let key_b64 = describe
        .signature
        .as_ref()
        .map(|s| s.public_key.clone())
        .ok_or_else(|| {
            RegistryError::SignatureInvalid(
                "unsigned describe cannot be trusted under Normal/Strict policy".into(),
            )
        })?;

    // 1. The signature must actually verify against the key it claims.
    let verifying_key = parse_verifying_key(&key_b64)?;
    greentic_extension_sdk_contract::verify_describe_with_key(describe, &verifying_key)
        .map_err(|e| RegistryError::SignatureInvalid(e.to_string()))?;

    // 2. Apply the trust anchor.
    let store = crate::trust_store::TrustStore::new(root);
    match policy {
        TrustPolicy::Loose => unreachable!("handled above"),
        TrustPolicy::Normal => store.pin_or_verify(&describe.metadata.id, &key_b64),
        TrustPolicy::Strict => {
            if store.is_trusted(&describe.metadata.id, &key_b64)? {
                // The production root key is not yet provisioned, so Strict is
                // anchored only by a prior TOFU pin (or an out-of-band entry) —
                // NOT a cert chain to a Greentic root. Make that explicit so a
                // user choosing Strict isn't misled into assuming root-anchored
                // authenticity (audit cycle-2; root provisioning = D.5).
                tracing::warn!(
                    name = %describe.metadata.id,
                    "TrustPolicy::Strict: publisher key is trust-on-first-use pinned, \
                     not anchored to a provisioned Greentic root key"
                );
                Ok(())
            } else {
                Err(RegistryError::UntrustedPublisher {
                    name: describe.metadata.id.clone(),
                })
            }
        }
    }
}

/// Read the raw `manifest.json` bytes from a `.gtxpack` zip, if present.
/// Read a zip entry with a hard ceiling on decompressed bytes.
///
/// `manifest.json` and `describe.json` are read *before*
/// `verify_archive_against_manifest`, and `describe.json` is excluded from the
/// ledger entirely — so neither was covered by the archive's zip-bomb caps.
/// A ~1 MB pack whose manifest inflates to tens of GB could OOM the client
/// before the consent prompt was ever shown.
fn read_entry_capped<R: std::io::Read>(
    entry: &mut R,
    name: &str,
) -> Result<Vec<u8>, RegistryError> {
    use std::io::Read as _;
    let cap = greentic_extension_sdk_contract::MAX_ENTRY_BYTES;
    let mut buf = Vec::new();
    // `take(cap + 1)` so an over-cap entry is detectable rather than silently
    // truncated to something that might still parse.
    entry
        .by_ref()
        .take(cap + 1)
        .read_to_end(&mut buf)
        .map_err(RegistryError::from)?;
    if buf.len() as u64 > cap {
        return Err(RegistryError::ArtifactTooLarge {
            limit: usize::try_from(cap).unwrap_or(usize::MAX),
        });
    }
    let _ = name;
    Ok(buf)
}

fn read_manifest_bytes(zip_bytes: &[u8]) -> Result<Option<Vec<u8>>, RegistryError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|e| RegistryError::Storage(format!("zip open: {e}")))?;
    match archive.by_name(greentic_extension_sdk_contract::MANIFEST_ENTRY_NAME) {
        Ok(mut entry) => Ok(Some(read_entry_capped(
            &mut entry,
            greentic_extension_sdk_contract::MANIFEST_ENTRY_NAME,
        )?)),
        Err(_) => Ok(None),
    }
}

/// Read and parse the `describe.json` actually contained in a `.gtxpack` zip —
/// the document that `extract_to_staging` writes to disk and the runtime reads
/// to grant permissions/capabilities. Distinct from the describe the registry
/// advertises via its metadata endpoint (see `verify_integrity`).
fn read_archive_describe(
    zip_bytes: &[u8],
) -> Result<greentic_extension_sdk_contract::DescribeJson, RegistryError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|e| RegistryError::Storage(format!("zip open: {e}")))?;
    let mut entry = archive
        .by_name(greentic_extension_sdk_contract::DESCRIBE_ENTRY_NAME)
        .map_err(|_| RegistryError::DescribeMissing)?;
    let buf = read_entry_capped(
        &mut entry,
        greentic_extension_sdk_contract::DESCRIBE_ENTRY_NAME,
    )?;
    let value: serde_json::Value = serde_json::from_slice(&buf)?;
    greentic_extension_sdk_contract::schema::validate_describe_json(&value)?;
    Ok(serde_json::from_value(value)?)
}

/// Enforce whole-archive integrity for every install: every file must match
/// the `manifest.json` ledger, and the describe must commit to that manifest
/// via `manifestSha256`.
///
/// Integrity is **not** authenticity. `Loose` waives the *signature* check
/// (`verify_authenticity` returns early), but it must NOT waive integrity — a
/// corrupt or tampered archive should never be extracted to disk, dev bypass
/// or not (audit P0-3). So this check runs unconditionally regardless of
/// `policy`; under `Loose` we emit a `tracing::warn!` to make the bypassed
/// *authenticity* boundary auditable while still proving the bytes are intact.
pub(crate) fn verify_integrity(
    artifact: &ExtensionArtifact,
    policy: TrustPolicy,
) -> Result<(), RegistryError> {
    if policy == TrustPolicy::Loose {
        tracing::warn!(
            name = %artifact.name,
            version = %artifact.version,
            "TrustPolicy::Loose: publisher signature is NOT verified; \
             whole-archive integrity is still enforced"
        );
    }
    let Some(manifest_bytes) = read_manifest_bytes(&artifact.bytes)? else {
        return Err(RegistryError::SignatureInvalid(
            "archive has no manifest.json — cannot verify whole-archive integrity".into(),
        ));
    };
    // Every entry hashes to what the ledger records (catches a swapped wasm).
    greentic_extension_sdk_contract::verify_archive_against_manifest(&artifact.bytes)
        .map_err(|e| RegistryError::SignatureInvalid(format!("manifest: {e}")))?;
    // The describe (covered by its signature) commits to exactly this manifest.
    greentic_extension_sdk_contract::verify_manifest_binding(&artifact.describe, &manifest_bytes)
        .map_err(|e| RegistryError::SignatureInvalid(e.to_string()))?;

    // Bind the describe we authenticated + consented to (served by the registry
    // metadata endpoint) to the describe.json actually inside the archive — the
    // one extracted to disk and read by the runtime to grant permissions and
    // capabilities. describe.json is deliberately excluded from the manifest
    // ledger, so none of the checks above tie these two together. Without this,
    // a tampered registry could serve a benign signed describe via /metadata
    // (passing authenticity + the consent prompt) while shipping an archive
    // whose describe.json declares broader permissions (audit cycle-2 H1).
    // Compare the canonical signing payloads so cosmetic JSON differences don't
    // matter but any semantic field (permissions, capabilities, runtime) does.
    let archive_describe = read_archive_describe(&artifact.bytes)?;
    let authenticated =
        greentic_extension_sdk_contract::canonical_signing_payload(&artifact.describe)?;
    let on_disk = greentic_extension_sdk_contract::canonical_signing_payload(&archive_describe)?;
    if authenticated != on_disk {
        return Err(RegistryError::DescribeMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_extension_sdk_contract::{ExtensionKind, build_manifest};
    use greentic_extension_sdk_testing::ExtensionFixtureBuilder;
    use std::io::Write as _;

    fn base_describe() -> greentic_extension_sdk_contract::DescribeJson {
        let fx = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.itest", "1.0.0")
            .offer("greentic:i/c", "1.0.0")
            .with_wasm(vec![])
            .build()
            .unwrap();
        serde_json::from_slice(&std::fs::read(&fx.describe_path).unwrap()).unwrap()
    }

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            for (name, body) in entries {
                w.start_file::<_, ()>(*name, zip::write::FileOptions::default())
                    .unwrap();
                w.write_all(body).unwrap();
            }
            w.finish().unwrap();
        }
        buf
    }

    fn artifact(
        describe: greentic_extension_sdk_contract::DescribeJson,
        bytes: Vec<u8>,
    ) -> ExtensionArtifact {
        ExtensionArtifact {
            name: "greentic.itest".into(),
            version: "1.0.0".into(),
            describe,
            bytes,
            signature: None,
        }
    }

    const WASM: &[u8] = b"\0asm\x01\x00\x00\x00";

    #[test]
    fn integrity_ok_for_bound_and_intact_archive() {
        let manifest = build_manifest(vec![("extension.wasm", WASM)]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut describe = base_describe();
        greentic_extension_sdk_contract::bind_manifest(&mut describe, &manifest_json);
        let describe_bytes = serde_json::to_vec(&describe).unwrap();
        let bytes = zip_bytes(&[
            ("extension.wasm", WASM),
            ("manifest.json", &manifest_json),
            ("describe.json", &describe_bytes),
        ]);
        verify_integrity(&artifact(describe, bytes), TrustPolicy::Normal).unwrap();
    }

    #[test]
    fn integrity_rejects_tampered_wasm() {
        let manifest = build_manifest(vec![("extension.wasm", WASM)]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut describe = base_describe();
        greentic_extension_sdk_contract::bind_manifest(&mut describe, &manifest_json);
        // Archive ships a different wasm than the manifest records.
        let bytes = zip_bytes(&[
            ("extension.wasm", b"evil"),
            ("manifest.json", &manifest_json),
        ]);
        assert!(verify_integrity(&artifact(describe, bytes), TrustPolicy::Normal).is_err());
    }

    #[test]
    fn integrity_rejects_missing_manifest_under_normal() {
        let describe = base_describe();
        let bytes = zip_bytes(&[("extension.wasm", WASM)]);
        assert!(verify_integrity(&artifact(describe, bytes), TrustPolicy::Normal).is_err());
    }

    #[test]
    fn integrity_rejects_binding_mismatch() {
        let manifest = build_manifest(vec![("extension.wasm", WASM)]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut describe = base_describe();
        // Bind to a *different* manifest, so the describe doesn't commit to the
        // one shipped in the archive.
        describe.manifest_sha256 = Some("0".repeat(64));
        let bytes = zip_bytes(&[("extension.wasm", WASM), ("manifest.json", &manifest_json)]);
        assert!(verify_integrity(&artifact(describe, bytes), TrustPolicy::Normal).is_err());
    }

    fn describe_json_bytes(describe: &greentic_extension_sdk_contract::DescribeJson) -> Vec<u8> {
        serde_json::to_vec(describe).unwrap()
    }

    #[test]
    fn integrity_rejects_archive_with_no_describe_json() {
        // An archive that ships a valid, bound manifest but no describe.json at
        // all must fail closed — there is nothing to confirm against the
        // authenticated describe (audit cycle-2 H1).
        let manifest = build_manifest(vec![("extension.wasm", WASM)]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut describe = base_describe();
        greentic_extension_sdk_contract::bind_manifest(&mut describe, &manifest_json);
        let bytes = zip_bytes(&[("extension.wasm", WASM), ("manifest.json", &manifest_json)]);
        let err = verify_integrity(&artifact(describe, bytes), TrustPolicy::Normal)
            .expect_err("archive without describe.json must be rejected");
        assert!(
            matches!(err, RegistryError::DescribeMissing),
            "expected DescribeMissing, got: {err:?}"
        );
    }

    #[test]
    fn integrity_rejects_archive_describe_that_differs_from_authenticated_describe() {
        // describe.json is deliberately excluded from the manifest ledger, so a
        // tampered registry can serve a benign signed describe via /metadata
        // while shipping an archive whose describe.json declares broader
        // permissions. verify_integrity must catch that the archive's describe
        // does not match the authenticated one (audit cycle-2 H1).
        let manifest = build_manifest(vec![("extension.wasm", WASM)]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();

        let mut authenticated = base_describe();
        greentic_extension_sdk_contract::bind_manifest(&mut authenticated, &manifest_json);

        // Same archive bytes + manifest binding, but the on-disk describe.json
        // asks for a secret the authenticated describe never declared.
        let mut tampered = authenticated.clone();
        tampered.runtime.permissions.secrets = vec!["greentic:secret/prod-db".into()];

        let bytes = zip_bytes(&[
            ("extension.wasm", WASM),
            ("manifest.json", &manifest_json),
            ("describe.json", &describe_json_bytes(&tampered)),
        ]);
        let err = verify_integrity(&artifact(authenticated, bytes), TrustPolicy::Normal)
            .expect_err("substituted describe must be rejected");
        assert!(
            matches!(err, RegistryError::DescribeMismatch),
            "expected DescribeMismatch, got: {err:?}"
        );
    }

    #[test]
    fn integrity_accepts_archive_describe_matching_authenticated_describe() {
        let manifest = build_manifest(vec![("extension.wasm", WASM)]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut describe = base_describe();
        greentic_extension_sdk_contract::bind_manifest(&mut describe, &manifest_json);
        let bytes = zip_bytes(&[
            ("extension.wasm", WASM),
            ("manifest.json", &manifest_json),
            ("describe.json", &describe_json_bytes(&describe)),
        ]);
        verify_integrity(&artifact(describe, bytes), TrustPolicy::Normal).unwrap();
    }

    #[test]
    fn integrity_enforced_even_under_loose_rejects_missing_manifest() {
        // Loose waives *authenticity* (signature), not *integrity*. An archive
        // with no manifest.json cannot prove its bytes are intact, so it must be
        // rejected even under the dev bypass (audit P0-3).
        let describe = base_describe();
        let bytes = zip_bytes(&[("extension.wasm", WASM)]); // no manifest at all
        assert!(verify_integrity(&artifact(describe, bytes), TrustPolicy::Loose).is_err());
    }

    #[test]
    fn integrity_ok_under_loose_for_bound_and_intact_archive() {
        // A correctly-bound, intact archive still passes under Loose — only the
        // signature check is skipped, the manifest ledger is honoured.
        let manifest = build_manifest(vec![("extension.wasm", WASM)]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut describe = base_describe();
        greentic_extension_sdk_contract::bind_manifest(&mut describe, &manifest_json);
        let describe_bytes = serde_json::to_vec(&describe).unwrap();
        let bytes = zip_bytes(&[
            ("extension.wasm", WASM),
            ("manifest.json", &manifest_json),
            ("describe.json", &describe_bytes),
        ]);
        verify_integrity(&artifact(describe, bytes), TrustPolicy::Loose).unwrap();
    }

    #[test]
    fn integrity_under_loose_rejects_tampered_wasm() {
        // Even with the signature waived, a swapped wasm must be caught by the
        // manifest ledger.
        let manifest = build_manifest(vec![("extension.wasm", WASM)]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut describe = base_describe();
        greentic_extension_sdk_contract::bind_manifest(&mut describe, &manifest_json);
        let bytes = zip_bytes(&[
            ("extension.wasm", b"evil"),
            ("manifest.json", &manifest_json),
        ]);
        assert!(verify_integrity(&artifact(describe, bytes), TrustPolicy::Loose).is_err());
    }

    /// A describe really signed by the ed25519 key derived from `seed`.
    fn signed_describe(seed: u8) -> greentic_extension_sdk_contract::DescribeJson {
        let sk = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
        let mut d = base_describe();
        greentic_extension_sdk_contract::sign_describe(&mut d, &sk).unwrap();
        d
    }

    #[test]
    fn authenticity_normal_pins_then_accepts_same_key() {
        let tmp = tempfile::TempDir::new().unwrap();
        let d = signed_describe(1);
        verify_authenticity(tmp.path(), &d, TrustPolicy::Normal).unwrap(); // pins
        verify_authenticity(tmp.path(), &d, TrustPolicy::Normal).unwrap(); // same key ok
    }

    #[test]
    fn authenticity_normal_rejects_changed_key() {
        let tmp = tempfile::TempDir::new().unwrap();
        verify_authenticity(tmp.path(), &signed_describe(1), TrustPolicy::Normal).unwrap();
        // Same extension id, signed by a different key.
        let other = signed_describe(2);
        assert!(matches!(
            verify_authenticity(tmp.path(), &other, TrustPolicy::Normal),
            Err(RegistryError::PublisherKeyChanged { .. })
        ));
    }

    #[test]
    fn authenticity_strict_rejects_untrusted_then_accepts_after_pin() {
        let tmp = tempfile::TempDir::new().unwrap();
        let d = signed_describe(1);
        // Strict with an empty trust store → rejected (no auto-pin).
        assert!(matches!(
            verify_authenticity(tmp.path(), &d, TrustPolicy::Strict),
            Err(RegistryError::UntrustedPublisher { .. })
        ));
        // Pin via a Normal install, then Strict accepts the same key.
        verify_authenticity(tmp.path(), &d, TrustPolicy::Normal).unwrap();
        verify_authenticity(tmp.path(), &d, TrustPolicy::Strict).unwrap();
    }

    #[test]
    fn authenticity_rejects_tampered_describe_and_pins_nothing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut d = signed_describe(1);
        // Mutate after signing → signature no longer valid.
        d.metadata.version = "9.9.9".into();
        assert!(verify_authenticity(tmp.path(), &d, TrustPolicy::Normal).is_err());
        // The bad key must NOT have been pinned.
        let store = crate::trust_store::TrustStore::new(tmp.path());
        assert!(store.pinned(&d.metadata.id).unwrap().is_none());
    }

    #[test]
    fn authenticity_skipped_under_loose() {
        let tmp = tempfile::TempDir::new().unwrap();
        verify_authenticity(tmp.path(), &base_describe(), TrustPolicy::Loose).unwrap();
    }

    #[test]
    fn authenticity_requires_signature_under_normal() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(verify_authenticity(tmp.path(), &base_describe(), TrustPolicy::Normal).is_err());
    }
}
