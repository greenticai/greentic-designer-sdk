use std::io::Cursor;

use greentic_ext_contract::ExtensionKind;

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
        let artifact = self.registry.fetch(name, version).await?;
        Self::verify_signature(&artifact, opts.trust_policy)?;
        self.install_artifact(&artifact, opts)
    }

    pub fn install_artifact(
        &self,
        artifact: &ExtensionArtifact,
        opts: InstallOptions,
    ) -> Result<(), RegistryError> {
        let kind = artifact.describe.kind;
        let (staging, final_dir) =
            self.storage
                .begin_install(kind, &artifact.name, &artifact.version)?;

        let result = Self::extract_to_staging(artifact, &staging);
        if result.is_err() {
            self.storage.abort_install(&staging);
            result?;
        }

        if kind == ExtensionKind::Provider {
            let post_result = post_install_provider(
                &staging,
                &artifact.describe,
                self.storage.root(),
                opts.force,
            );
            if post_result.is_err() {
                self.storage.abort_install(&staging);
                post_result?;
            }
        }

        self.storage.commit_install(&staging, &final_dir)?;
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
                greentic_ext_contract::verify_describe(&artifact.describe)
                    .map_err(|e| RegistryError::SignatureInvalid(e.to_string()))
            }
        }
    }

    pub fn uninstall(
        &self,
        kind: greentic_ext_contract::ExtensionKind,
        name: &str,
        version: &str,
    ) -> Result<(), RegistryError> {
        self.storage.remove_extension(kind, name, version)
    }
}
