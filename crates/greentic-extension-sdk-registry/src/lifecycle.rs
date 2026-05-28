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
        Self::verify_signature(&artifact, opts.trust_policy)?;
        verify_integrity(&artifact, opts.trust_policy)?;
        if opts.trust_policy == TrustPolicy::Normal {
            tofu_verify(self.storage.root(), &artifact.describe)?;
        }
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
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| RegistryError::Storage(format!("zip entry: {e}")))?;
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

    fn verify_signature(
        artifact: &ExtensionArtifact,
        policy: TrustPolicy,
    ) -> Result<(), RegistryError> {
        match policy {
            TrustPolicy::Loose => Ok(()),
            TrustPolicy::Strict | TrustPolicy::Normal => {
                greentic_extension_sdk_contract::verify_describe(&artifact.describe)
                    .map_err(|e| RegistryError::SignatureInvalid(e.to_string()))
            }
        }
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

/// Trust-on-first-use check (`Normal` policy): pin the publisher key that signed
/// this describe on first install of the id, and require the same key on later
/// installs. Requires a signed describe (the key to pin comes from its signature).
fn tofu_verify(
    root: &std::path::Path,
    describe: &greentic_extension_sdk_contract::DescribeJson,
) -> Result<(), RegistryError> {
    let key = describe
        .signature
        .as_ref()
        .map(|s| s.public_key.as_str())
        .ok_or_else(|| {
            RegistryError::SignatureInvalid(
                "unsigned describe cannot be trusted under Normal policy (TOFU needs a signature)"
                    .into(),
            )
        })?;
    crate::trust_store::TrustStore::new(root).pin_or_verify(&describe.metadata.id, key)
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

/// Enforce whole-archive integrity for trusted installs: every file must match
/// the `manifest.json` ledger, and the (signed) describe must commit to that
/// manifest via `manifestSha256`. Skipped under `Loose` (the dev bypass), which
/// also skips signature verification.
fn verify_integrity(
    artifact: &ExtensionArtifact,
    policy: TrustPolicy,
) -> Result<(), RegistryError> {
    if policy == TrustPolicy::Loose {
        return Ok(());
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
        let bytes = zip_bytes(&[("extension.wasm", WASM), ("manifest.json", &manifest_json)]);
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

    #[test]
    fn integrity_skipped_under_loose() {
        let describe = base_describe();
        let bytes = zip_bytes(&[("extension.wasm", WASM)]); // no manifest at all
        verify_integrity(&artifact(describe, bytes), TrustPolicy::Loose).unwrap();
    }

    fn signed_with(describe: &mut greentic_extension_sdk_contract::DescribeJson, pubkey: &str) {
        describe.signature = Some(greentic_extension_sdk_contract::Signature {
            algorithm: greentic_extension_sdk_contract::SignatureAlgorithm::Ed25519,
            public_key: pubkey.into(),
            value: "sig".into(),
            key_id: None,
        });
    }

    #[test]
    fn tofu_pins_then_rejects_changed_key() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut d = base_describe();
        signed_with(&mut d, "PUBKEY1");
        tofu_verify(tmp.path(), &d).unwrap(); // first use pins
        tofu_verify(tmp.path(), &d).unwrap(); // same key accepted

        signed_with(&mut d, "PUBKEY2");
        assert!(
            matches!(
                tofu_verify(tmp.path(), &d),
                Err(RegistryError::PublisherKeyChanged { .. })
            ),
            "a different publisher key must be rejected"
        );
    }

    #[test]
    fn tofu_requires_a_signature() {
        let tmp = tempfile::TempDir::new().unwrap();
        let d = base_describe(); // signature: None
        assert!(tofu_verify(tmp.path(), &d).is_err());
    }
}
