//! Install lifecycle orchestration: fetch → verify → consent → stage → commit.
//!
//! Verification policy lives in [`crate::verify`]; archive extraction and its
//! filesystem guards live in [`crate::extract`]. This module only sequences
//! those steps and owns rollback on failure.

use greentic_extension_sdk_contract::ExtensionKind;

use crate::error::RegistryError;
use crate::extract::extract_to_staging;
use crate::provider_install::post_install_provider;
use crate::registry::ExtensionRegistry;
use crate::storage::Storage;
use crate::types::ExtensionArtifact;
use crate::verify::{verify_authenticity, verify_integrity};

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
        // Every check below this line is self-referential to the served
        // describe: integrity binds the archive to it, authenticity binds a
        // signature to it. None of them notice if the registry answered with a
        // *different* extension than the one requested. Bind the served
        // identity to the request first, and do it before `verify_authenticity`
        // so a substituted publisher key is never TOFU-pinned.
        assert_served_identity(name, version, &artifact)?;
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

        let result = extract_to_staging(artifact, &staging);
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
            if let Some(dest) = provider_gtpack_dest
                && let Err(cleanup_err) = std::fs::remove_file(&dest)
            {
                // Don't mask the commit error, but never fail silently: an
                // orphaned gtpack accumulates across repeated failed installs.
                tracing::warn!(
                    path = %dest.display(),
                    error = %cleanup_err,
                    "failed to remove provider gtpack while rolling back a failed commit"
                );
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
    pub fn uninstall(
        &self,
        kind: greentic_extension_sdk_contract::ExtensionKind,
        name: &str,
        version: &str,
    ) -> Result<(), RegistryError> {
        self.storage.remove_extension(kind, name, version)
    }
}

/// Reject an artifact whose identity does not match what the caller asked for.
///
/// `fetch` implementations build `ExtensionArtifact::{name, version}` from the
/// *served* `describe.metadata`, discarding the requested coordinates. A
/// registry can therefore answer a request for `a@1.0.0` with `b@9.9.9`: every
/// downstream check still passes, because they all validate the served
/// describe against itself. The install would land under `b`, pin `b`'s
/// publisher key, and — in `update::upgrade` — delete `a` on the way out.
fn assert_served_identity(
    requested_name: &str,
    requested_version: &str,
    artifact: &ExtensionArtifact,
) -> Result<(), RegistryError> {
    let served_name = artifact.describe.metadata.id.as_str();
    let served_version = artifact.describe.metadata.version.as_str();
    if served_name == requested_name && served_version == requested_version {
        return Ok(());
    }
    Err(RegistryError::IdentityMismatch {
        requested_name: requested_name.to_string(),
        requested_version: requested_version.to_string(),
        served_name: served_name.to_string(),
        served_version: served_version.to_string(),
    })
}
