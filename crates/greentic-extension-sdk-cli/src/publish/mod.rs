//! gtdx publish: build + validate + pack + publish orchestration.
mod backend;
mod error;
pub mod receipt;
mod signing_key;
#[cfg(test)]
mod tests;
pub mod validator;

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use greentic_extension_sdk_contract::DescribeJson;
use greentic_extension_sdk_registry::RegistryError;
use greentic_extension_sdk_registry::publish::{PublishRequest, SignatureBlob};
use greentic_extension_sdk_registry::registry::ExtensionRegistry;

use crate::dev::builder::{Profile, run_build};
use crate::dev::packer::build_pack_with_key;
use crate::publish::receipt::{PublishReceiptJson, write_receipt};
use crate::publish::validator::{format_errors, validate_for_publish};
use backend::{Backend, resolve_backend};
pub use error::PublishError;
use error::io_err;
use signing_key::resolve_signing_key;

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct PublishConfig {
    pub project_dir: PathBuf,
    pub registry_uri: String,
    pub home: PathBuf,
    pub dist_dir: PathBuf,
    pub profile: Profile,
    pub dry_run: bool,
    pub force: bool,
    pub sign: bool,
    pub key_id: Option<String>,
    /// Explicit PKCS8 PEM signing-key file (takes precedence over `key_id`).
    pub key_path: Option<PathBuf>,
    /// Env var holding a PKCS8 PEM signing key (fallback when no file/key-id).
    pub key_env: String,
    pub version_override: Option<String>,
    pub trust_policy: String,
    pub verify_only: bool,
    /// Explicit bearer/PAT token for `oci://...` registries. When `None`,
    /// `resolve_backend` falls back to `GHCR_TOKEN` / `GITHUB_TOKEN` /
    /// `OCI_TOKEN` env vars, then anonymous.
    pub oci_token: Option<String>,
    /// Pre-built `wasm32-wasip2` component to pack instead of running
    /// `cargo component build` from `project_dir`. Lets an externally produced
    /// component (e.g. a generated MCP component) be packed + signed + published
    /// through the same path. When `None`, the project is built from source.
    pub wasm_override: Option<PathBuf>,
}

#[derive(Debug)]
pub enum PublishOutcome {
    DryRun {
        artifact: PathBuf,
        sha256: String,
        registry: String,
    },
    VerifyOnly {
        ext_id: String,
        version: String,
        registry: String,
    },
    Published {
        ext_id: String,
        version: String,
        sha256: String,
        artifact: PathBuf,
        receipt_path: PathBuf,
        signed: bool,
        registry_url: String,
    },
}

/// Resolve the component `.wasm` to pack: an externally-built artifact when
/// `--wasm` is given (skips `cargo component build`), otherwise a fresh build
/// of the project. Kept separate from `run_publish` so the override branch is
/// unit-testable without invoking the toolchain.
fn resolve_publish_wasm(cfg: &PublishConfig) -> Result<PathBuf, PublishError> {
    if let Some(wasm) = &cfg.wasm_override {
        if !wasm.is_file() {
            return Err(PublishError::Build(format!(
                "--wasm path is not a file: {}",
                wasm.display()
            )));
        }
        return Ok(wasm.clone());
    }
    let build = run_build(&cfg.project_dir, cfg.profile)
        .map_err(|e| PublishError::Build(format!("cargo component build: {e}")))?;
    Ok(build.wasm_path)
}

#[allow(clippy::too_many_lines)]
pub async fn run_publish(cfg: &PublishConfig) -> Result<PublishOutcome, PublishError> {
    // 0. `--trust strict` asserts a signed artifact; refuse to publish one
    //    unsigned so the flag actually gates behavior instead of only labeling
    //    the receipt (audit cycle-1 P2). Checked first so it fails fast before
    //    any build/pack work.
    if cfg.trust_policy == "strict" && !cfg.sign {
        return Err(PublishError::Other(anyhow::anyhow!(
            "--trust strict requires a signature; pass --sign with --key / --key-id / --key-env"
        )));
    }

    // 1. Load + schema-validate describe.json via ext-contract.
    let describe_path = cfg.project_dir.join("describe.json");
    let describe_bytes = std::fs::read(&describe_path).map_err(io_err)?;
    let describe_value: serde_json::Value = serde_json::from_slice(&describe_bytes)
        .map_err(|e| PublishError::DescribeInvalid(format!("parse describe.json: {e}")))?;
    greentic_extension_sdk_contract::schema::validate_describe_json(&describe_value)
        .map_err(|e| PublishError::DescribeInvalid(format!("describe.json schema: {e}")))?;
    let mut describe: DescribeJson = serde_json::from_value(describe_value)
        .map_err(|e| PublishError::DescribeInvalid(format!("parse describe.json: {e}")))?;
    if let Some(v) = &cfg.version_override {
        describe.metadata.version = v.clone();
    }

    // 2. Business-rule validator (aggregated).
    if let Err(errors) = validate_for_publish(&describe) {
        return Err(PublishError::DescribeInvalid(format_errors(&errors)));
    }

    // 3. Resolve registry root.
    let backend = resolve_backend(&cfg.registry_uri, &cfg.home, cfg.oci_token.as_deref())
        .map_err(|e| PublishError::Other(anyhow::anyhow!("{e}")))?;

    if cfg.verify_only {
        return verify_only(&backend, &describe, cfg.force).await;
    }

    // 4. Resolve the component wasm: an externally-built artifact (`--wasm`,
    //    e.g. a generated MCP component) or a fresh `cargo component build`.
    let wasm_path = resolve_publish_wasm(cfg)?;

    // 5. Resolve the signing key first (if requested), so the *pack's*
    //    describe.json is manifest-bound and signed — the installable artifact
    //    carries the authenticated descriptor, not just the registry metadata.
    let key_and_id = if cfg.sign {
        Some(resolve_signing_key(cfg).map_err(|e| PublishError::Other(anyhow::anyhow!("{e}")))?)
    } else {
        None
    };

    // 6. Pack deterministic .gtxpack (manifest-bound; signed when a key is set).
    let staging_pack = cfg.project_dir.join("dist/publish-staging.gtxpack");
    let info = build_pack_with_key(
        &cfg.project_dir,
        &wasm_path,
        &staging_pack,
        key_and_id.as_ref().map(|(key, _)| key),
    )
    .map_err(|e| PublishError::Other(anyhow::anyhow!("{e}")))?;
    let pack_bytes = std::fs::read(&staging_pack).map_err(io_err)?;

    // `build_pack_with_key` may have persisted freshly-computed sha256 digests
    // back into the project's describe.json on disk (`fill_self_contained_hashes`
    // in dev/packer/mod.rs) — real, wanted behavior for `gtdx dev` / a real
    // `gtdx publish`. But `--dry-run` is documented as "skip registry write" and
    // must not leave the working tree dirty either: restore describe.json to
    // exactly the bytes it had before this run so a dry run never mutates the
    // project. The pack + reported sha256 above are still computed for real.
    if cfg.dry_run {
        std::fs::write(&describe_path, &describe_bytes).map_err(io_err)?;
    }

    // The pack's describe.json is now authoritative (bound + signed); use it for
    // the registry metadata so the two match exactly.
    describe = serde_json::from_slice(&info.describe_bytes)
        .map_err(|e| PublishError::Other(anyhow::anyhow!("parse signed describe: {e}")))?;

    let signature = match (&key_and_id, &describe.signature) {
        (Some((_, key_id)), Some(sig)) => Some(SignatureBlob {
            algorithm: match sig.algorithm {
                greentic_extension_sdk_contract::SignatureAlgorithm::Ed25519 => {
                    "ed25519".to_string()
                }
            },
            public_key: sig.public_key.clone(),
            value: sig.value.clone(),
            key_id: key_id.clone(),
        }),
        _ => None,
    };

    if cfg.dry_run {
        return Ok(PublishOutcome::DryRun {
            artifact: staging_pack,
            sha256: info.sha256,
            registry: backend_registry_label(&backend),
        });
    }

    // 7. Publish through the registry trait.
    let req = PublishRequest {
        ext_id: describe.metadata.id.clone(),
        ext_name: describe.metadata.name.clone(),
        version: describe.metadata.version.clone(),
        kind: describe.kind,
        artifact_bytes: pack_bytes.clone(),
        artifact_sha256: info.sha256.clone(),
        describe: describe.clone(),
        signature: signature.clone(),
        force: cfg.force,
    };

    let receipt = match backend.publish(req).await {
        Ok(r) => r,
        Err(e) => {
            // Don't leave the transient staging pack behind on a failed publish
            // (auth/network/conflict); it could be mistaken for a real artifact
            // and accumulates across retries (audit cycle-2 P3).
            let _ = std::fs::remove_file(&staging_pack);
            return Err(map_registry_err(e));
        }
    };

    // 8. Also copy into local ./dist/ with the canonical name, and drop the
    //    transient staging pack so it doesn't linger.
    let final_dist = write_canonical_dist(
        &staging_pack,
        &cfg.dist_dir,
        &describe.metadata.name,
        &describe.metadata.version,
        &pack_bytes,
    )
    .map_err(io_err)?;

    let receipt_json = PublishReceiptJson {
        artifact: final_dist
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("pack.gtxpack")
            .to_string(),
        sha256: info.sha256,
        registry: receipt.url.clone(),
        published_at: receipt.published_at,
        trust_policy: cfg.trust_policy.clone(),
        signed: receipt.signed,
        signing_known_limitations: None,
    };
    let receipt_path = write_receipt(
        &cfg.dist_dir,
        &describe.metadata.id,
        &describe.metadata.version,
        &receipt_json,
    )
    .map_err(io_err)?;

    Ok(PublishOutcome::Published {
        ext_id: describe.metadata.id,
        version: describe.metadata.version,
        sha256: receipt_json.sha256,
        artifact: final_dist,
        receipt_path,
        signed: receipt.signed,
        registry_url: receipt.url,
    })
}

fn map_registry_err(e: RegistryError) -> PublishError {
    match e {
        RegistryError::VersionExists { existing_sha } => {
            PublishError::VersionExists(format!("version already exists (sha256={existing_sha})"))
        }
        RegistryError::AuthRequired(m) | RegistryError::AuthFailed(m) => {
            PublishError::AuthRequired(m)
        }
        RegistryError::NotImplemented { hint } => PublishError::NotImplemented(hint),
        RegistryError::Io(io) => PublishError::Io(io.to_string()),
        RegistryError::Storage(s) => PublishError::RegistryNotWritable(s),
        other => PublishError::Other(anyhow::anyhow!("{other}")),
    }
}

async fn verify_only(
    backend: &Backend,
    describe: &DescribeJson,
    force: bool,
) -> Result<PublishOutcome, PublishError> {
    let id = describe.metadata.id.clone();
    let version = describe.metadata.version.clone();
    match backend {
        Backend::Local(r) => {
            let ver_dir = r.root_path().join(&id).join(&version);
            if ver_dir.exists() && !force {
                return Err(PublishError::VersionExists(format!(
                    "version {version} already exists at {}",
                    ver_dir.display()
                )));
            }
            Ok(PublishOutcome::VerifyOnly {
                ext_id: id,
                version,
                registry: r.root_path().display().to_string(),
            })
        }
        Backend::Store(r) => {
            // Real server-side conflict probe: list the published versions and
            // fail if this one already exists (audit N3 — previously a no-op
            // success that printed "slot free" without contacting the server).
            let existing = r.list_versions(&id).await.map_err(map_registry_err)?;
            if existing.contains(&version) && !force {
                return Err(PublishError::VersionExists(format!(
                    "version {version} already exists in {}",
                    r.base_url()
                )));
            }
            Ok(PublishOutcome::VerifyOnly {
                ext_id: id,
                version,
                registry: r.base_url().to_string(),
            })
        }
        Backend::Oci(_) => {
            // OCI has no real conflict probe here — `list_versions` is an
            // empty-list stub, so reusing it would report every slot as free.
            // Returning a success would be a false "slot free"; surface
            // NotImplemented (distinct exit code) instead (audit N3).
            Err(PublishError::NotImplemented(
                "verify-only conflict probe is not implemented for OCI registries".into(),
            ))
        }
    }
}

fn backend_registry_label(backend: &Backend) -> String {
    match backend {
        Backend::Local(r) => r.root_path().display().to_string(),
        Backend::Store(r) => r.base_url().to_string(),
        Backend::Oci(_) => "oci-registry".to_string(),
    }
}

/// Write the published bytes to `<dist_dir>/<name>-<version>.gtxpack` and remove
/// the transient `staging` pack so it doesn't linger in `./dist` after publish.
///
/// `name` is free-form `describe.json` metadata (display text — an author
/// can put a `/` in it, e.g. "Topic / scope guardrail") and is sanitized via
/// [`greentic_extension_sdk_contract::safe_pack_filename`] before it becomes
/// a path component; an unsanitized `/` would otherwise be read as a path
/// separator and target a nonexistent nested directory.
fn write_canonical_dist(
    staging: &Path,
    dist_dir: &Path,
    name: &str,
    version: &str,
    bytes: &[u8],
) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dist_dir)
        .with_context(|| format!("create dist dir {}", dist_dir.display()))?;
    let pack_name = greentic_extension_sdk_contract::safe_pack_filename(name, version);
    let final_dist = dist_dir.join(&pack_name);
    std::fs::write(&final_dist, bytes)
        .with_context(|| format!("write {}", final_dist.display()))?;
    let _ = std::fs::remove_file(staging);
    Ok(final_dist)
}
