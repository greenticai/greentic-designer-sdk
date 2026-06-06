use std::io::Cursor;

use greentic_extension_sdk_contract::ExtensionKind;

use crate::error::RegistryError;
use crate::provider_install::post_install_provider;
use crate::registry::ExtensionRegistry;
use crate::storage::Storage;
use crate::types::ExtensionArtifact;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustPolicy {
    Strict,
    Normal,
    Loose,
}

#[derive(Debug, Clone, Copy)]
pub struct InstallOptions {
    pub trust_policy: TrustPolicy,
    pub accept_permissions: bool,
    pub force: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            trust_policy: TrustPolicy::Normal,
            accept_permissions: false,
            force: false,
        }
    }
}

pub struct Installer<'a, R: ExtensionRegistry + ?Sized> {
    storage: Storage,
    registry: &'a R,
}

impl<'a, R: ExtensionRegistry + ?Sized> Installer<'a, R> {
    pub fn new(storage: Storage, registry: &'a R) -> Self {
        Self { storage, registry }
    }

    pub async fn install(
        &self,
        name: &str,
        version: &str,
        opts: InstallOptions,
    ) -> Result<(), RegistryError> {
        // Best-effort yanked check: refuse a yanked version unless forced.
        // Skipped silently for registries that don't support metadata
        // introspection (e.g. OCI), which return an error here.
        if let Ok(metadata) = self.registry.metadata(name, version).await
            && metadata.yanked
            && !opts.force
        {
            return Err(RegistryError::Yanked {
                name: name.into(),
                version: version.into(),
            });
        }
        let artifact = self.registry.fetch(name, version).await?;
        verify_integrity(&artifact, opts.trust_policy)?;
        verify_authenticity(self.storage.root(), &artifact.describe, opts.trust_policy)?;
        self.install_artifact(&artifact, opts)
    }

    /// Install an already-fetched artifact, prompting for permission consent
    /// via the interactive prompt. See [`Self::install_artifact_with_confirm`]
    /// for the testable, injectable variant.
    pub fn install_artifact(
        &self,
        artifact: &ExtensionArtifact,
        opts: InstallOptions,
    ) -> Result<(), RegistryError> {
        self.install_artifact_with_confirm(artifact, opts, crate::prompt::confirm_install)
    }

    /// Install an already-fetched artifact, deciding permission consent via the
    /// supplied `confirm` callback (`(&describe, accept_permissions) -> bool`).
    /// The consent gate runs *before* anything is written to disk: if `confirm`
    /// returns `false`, no files are extracted and [`RegistryError::PermissionDenied`]
    /// is returned. Tests inject a stub here to avoid the interactive prompt.
    pub fn install_artifact_with_confirm<F>(
        &self,
        artifact: &ExtensionArtifact,
        opts: InstallOptions,
        confirm: F,
    ) -> Result<(), RegistryError>
    where
        F: FnOnce(&greentic_extension_sdk_contract::DescribeJson, bool) -> bool,
    {
        if !confirm(&artifact.describe, opts.accept_permissions) {
            return Err(RegistryError::PermissionDenied {
                name: artifact.name.clone(),
                version: artifact.version.clone(),
            });
        }

        let kind = artifact.describe.kind;
        let (staging, final_dir) =
            self.storage
                .begin_install(kind, &artifact.name, &artifact.version)?;

        let result = Self::extract_to_staging(artifact, &staging);
        if result.is_err() {
            self.storage.abort_install(&staging);
            result?;
        }

        let mut provider_gtpack_dest: Option<std::path::PathBuf> = None;
        if kind == ExtensionKind::Provider {
            match post_install_provider(
                &staging,
                &artifact.describe,
                self.storage.root(),
                opts.force,
            ) {
                Ok(dest) => provider_gtpack_dest = Some(dest),
                Err(e) => {
                    self.storage.abort_install(&staging);
                    return Err(e);
                }
            }
        }

        if let Err(e) = self.storage.commit_install(&staging, &final_dir) {
            // Roll back the provider gtpack copied into the gtdx dir so a failed
            // commit does not leave a half-installed provider behind.
            self.storage.abort_install(&staging);
            if let Some(dest) = provider_gtpack_dest {
                let _ = std::fs::remove_file(dest);
            }
            return Err(e);
        }
        tracing::info!(
            name = %artifact.name,
            version = %artifact.version,
            kind = ?kind,
            "extension installed"
        );
        Ok(())
    }

    fn extract_to_staging(
        artifact: &ExtensionArtifact,
        staging: &std::path::Path,
    ) -> Result<(), RegistryError> {
        let cursor = Cursor::new(&artifact.bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| RegistryError::Storage(format!("zip open: {e}")))?;
        let mut seen: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| RegistryError::Storage(format!("zip entry: {e}")))?;

            // Reject symlink entries outright. The current writer (io::copy)
            // materializes them as regular files, but a symlink-aware unpacker
            // would let `describe.json -> /etc/...` or a cross-extension link
            // escape staging. Fail closed regardless of the writer (audit N7).
            if let Some(mode) = entry.unix_mode()
                && (mode & 0o17_0000) == 0o12_0000
            {
                return Err(RegistryError::Storage(format!(
                    "zip entry is a symlink, refusing to extract: {}",
                    entry.name()
                )));
            }

            let out_path = staging.join(entry.mangled_name());
            // Defense in depth: reject any entry whose resolved path
            // contains a `..` component — mangled_name() already strips
            // leading slashes and `..`, so this should never fire in practice.
            if out_path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(RegistryError::Storage(format!(
                    "zip entry escapes staging: {}",
                    out_path.display()
                )));
            }
            // Reject a second entry resolving to a path already written, so a
            // later entry can't overwrite an earlier (verified) one (audit N7).
            if !entry.is_dir() && !seen.insert(out_path.clone()) {
                return Err(RegistryError::Storage(format!(
                    "duplicate zip entry path: {}",
                    out_path.display()
                )));
            }
            if entry.is_dir() {
                std::fs::create_dir_all(&out_path)?;
                continue;
            }
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
        Ok(())
    }

    pub fn uninstall(
        &self,
        kind: greentic_extension_sdk_contract::ExtensionKind,
        name: &str,
        version: &str,
    ) -> Result<(), RegistryError> {
        self.storage.remove_extension(kind, name, version)
    }
}

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
fn verify_authenticity(
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
fn read_manifest_bytes(zip_bytes: &[u8]) -> Result<Option<Vec<u8>>, RegistryError> {
    use std::io::Read as _;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|e| RegistryError::Storage(format!("zip open: {e}")))?;
    match archive.by_name(greentic_extension_sdk_contract::MANIFEST_ENTRY_NAME) {
        Ok(mut entry) => {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            Ok(Some(buf))
        }
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
    use std::io::Read as _;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|e| RegistryError::Storage(format!("zip open: {e}")))?;
    let mut entry = archive
        .by_name(greentic_extension_sdk_contract::DESCRIBE_ENTRY_NAME)
        .map_err(|_| RegistryError::DescribeMissing)?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
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
fn verify_integrity(
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
    fn extract_rejects_symlink_entries() {
        // A symlink entry must be refused so it can never escape staging,
        // independent of how the writer materializes it (audit N7).
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            w.add_symlink::<_, _, ()>("link", "/etc/passwd", zip::write::FileOptions::default())
                .unwrap();
            w.finish().unwrap();
        }
        let staging = tempfile::TempDir::new().unwrap();
        let err = Installer::<crate::local::LocalFilesystemRegistry>::extract_to_staging(
            &artifact(base_describe(), buf),
            staging.path(),
        )
        .expect_err("symlink entry must be rejected");
        assert!(matches!(err, RegistryError::Storage(m) if m.contains("symlink")));
    }

    #[test]
    fn extract_rejects_duplicate_entry_paths() {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::FileOptions::<()>::default();
            // Two distinct entry names that resolve to the same staging path.
            w.start_file("dup.txt", opts).unwrap();
            w.write_all(b"first").unwrap();
            w.start_file("./dup.txt", opts).unwrap();
            w.write_all(b"second").unwrap();
            w.finish().unwrap();
        }
        let staging = tempfile::TempDir::new().unwrap();
        let err = Installer::<crate::local::LocalFilesystemRegistry>::extract_to_staging(
            &artifact(base_describe(), buf),
            staging.path(),
        )
        .expect_err("duplicate path must be rejected");
        assert!(matches!(err, RegistryError::Storage(m) if m.contains("duplicate")));
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
