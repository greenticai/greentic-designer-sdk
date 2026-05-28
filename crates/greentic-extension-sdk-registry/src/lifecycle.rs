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
