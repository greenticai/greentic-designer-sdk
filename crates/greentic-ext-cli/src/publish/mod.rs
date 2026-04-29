//! gtdx publish: build + validate + pack + publish orchestration.

pub mod receipt;
pub mod validator;

use std::path::{Path, PathBuf};

use greentic_ext_contract::DescribeJson;
use greentic_ext_registry::RegistryError;
use greentic_ext_registry::local::LocalFilesystemRegistry;
use greentic_ext_registry::publish::{PublishRequest, SignatureBlob};
use greentic_ext_registry::registry::ExtensionRegistry;

use crate::dev::builder::{Profile, run_build};
use crate::dev::packer::build_pack;
use crate::publish::receipt::{PublishReceiptJson, write_receipt};
use crate::publish::validator::{format_errors, validate_for_publish};

use greentic_ext_registry::credentials::Credentials;
use greentic_ext_registry::oci::OciRegistry;
use greentic_ext_registry::store::GreenticStoreRegistry;

/// Typed publish error with spec §9 exit codes.
#[derive(Debug)]
pub enum PublishError {
    /// describe.json missing, malformed, schema-invalid, or business-rule invalid. Exit 2.
    DescribeInvalid(String),
    /// `cargo component build` failed. Exit 70.
    Build(String),
    /// Target version already exists and `--force` was not supplied. Exit 10.
    VersionExists(String),
    /// Registry demands credentials but none were provided. Exit 20.
    AuthRequired(String),
    /// Registry refused the write (e.g. read-only / permissions). Exit 30.
    RegistryNotWritable(String),
    /// Backend path not yet implemented (Phase 2 stubs). Exit 50.
    NotImplemented(String),
    /// Filesystem I/O or network I/O failure. Exit 74.
    Io(String),
    /// Catch-all for unexpected errors. Exit 1.
    Other(anyhow::Error),
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PublishError::DescribeInvalid(m)
            | PublishError::Build(m)
            | PublishError::VersionExists(m)
            | PublishError::AuthRequired(m)
            | PublishError::RegistryNotWritable(m)
            | PublishError::NotImplemented(m)
            | PublishError::Io(m) => write!(f, "{m}"),
            PublishError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PublishError {}

impl PublishError {
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            PublishError::DescribeInvalid(_) => 2,
            PublishError::VersionExists(_) => 10,
            PublishError::AuthRequired(_) => 20,
            PublishError::RegistryNotWritable(_) => 30,
            PublishError::NotImplemented(_) => 50,
            PublishError::Build(_) => 70,
            PublishError::Io(_) => 74,
            PublishError::Other(_) => 1,
        }
    }
}

fn io_err<E: std::fmt::Display>(e: E) -> PublishError {
    PublishError::Io(e.to_string())
}

enum Backend {
    Local(LocalFilesystemRegistry),
    Store(GreenticStoreRegistry),
    Oci(OciRegistry),
}

impl Backend {
    async fn publish(
        &self,
        req: greentic_ext_registry::publish::PublishRequest,
    ) -> Result<greentic_ext_registry::publish::PublishReceipt, greentic_ext_registry::RegistryError>
    {
        match self {
            Backend::Local(r) => r.publish(req).await,
            Backend::Store(r) => r.publish(req).await,
            Backend::Oci(r) => r.publish(req).await,
        }
    }
}

fn resolve_backend(
    uri: &str,
    home: &Path,
    oci_token_override: Option<&str>,
) -> anyhow::Result<Backend> {
    if uri == "local" {
        let root = home.join("registries/local");
        return Ok(Backend::Local(LocalFilesystemRegistry::new(
            "publish-local",
            root,
        )));
    }
    if let Some(rest) = uri.strip_prefix("file://") {
        let root = std::path::PathBuf::from(rest);
        return Ok(Backend::Local(LocalFilesystemRegistry::new("file", root)));
    }
    if let Some(rest) = uri.strip_prefix("oci://") {
        return build_oci_backend(rest, oci_token_override);
    }

    let cfg = greentic_ext_registry::config::load(&home.join("config.toml"))
        .map_err(|e| anyhow::anyhow!("load config: {e}"))?;
    let entry = cfg
        .registries
        .iter()
        .find(|e| e.name == uri)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no registry named '{uri}' in {}/config.toml. Add one with: gtdx registries add {uri} <url>",
                home.display()
            )
        })?;

    let token = resolve_token(home, entry);
    Ok(Backend::Store(GreenticStoreRegistry::new(
        &entry.name,
        &entry.url,
        token,
    )))
}

/// Parse `oci://<host>/<namespace>[/<artifact-name>]` into an `OciRegistry`.
///
/// Two forms are accepted:
/// - `oci://ghcr.io/myorg` — namespace only; the final artifact name is
///   taken from `PublishRequest.ext_name` per-publish (one GHCR package per
///   extension).
/// - `oci://ghcr.io/myorg/my-package` — fully qualified; every publish from
///   this URI targets the same `my-package` (different tags per version).
///
/// Auth resolution:
///
///   1. `--oci-token` CLI flag (explicit override)
///   2. `GHCR_TOKEN` env
///   3. `GITHUB_TOKEN` env (CI-friendly — `actions/checkout@v4` exports this)
///   4. `OCI_TOKEN` env (generic)
///   5. anonymous (public pulls only; push will 401)
fn build_oci_backend(spec: &str, oci_token_override: Option<&str>) -> anyhow::Result<Backend> {
    let (host, rest) = spec.split_once('/').ok_or_else(|| {
        anyhow::anyhow!(
            "oci:// URI must include at least a namespace: oci://<host>/<namespace>[/<name>]"
        )
    })?;
    if host.is_empty() {
        anyhow::bail!("oci:// URI missing host: {spec}");
    }

    let (namespace, artifact_name) = match rest.rsplit_once('/') {
        Some((ns, name)) if !ns.is_empty() && !name.is_empty() => {
            (ns.to_string(), Some(name.to_string()))
        }
        _ => (rest.to_string(), None),
    };
    if namespace.is_empty() {
        anyhow::bail!("oci:// URI namespace is empty: {spec}");
    }

    let token = oci_token_override
        .map(str::to_string)
        .or_else(|| non_empty_env("GHCR_TOKEN"))
        .or_else(|| non_empty_env("GITHUB_TOKEN"))
        .or_else(|| non_empty_env("OCI_TOKEN"));

    let auth = token.map(|t| oci_basic_auth_for(host, t));
    let mut reg = OciRegistry::new(format!("oci-{host}"), host, namespace, auth);
    if let Some(name) = artifact_name {
        reg = reg.with_artifact_name(name);
    }
    Ok(Backend::Oci(reg))
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

/// OCI registries expect `(username, password)` basic auth. GHCR convention
/// is `(<any-username>, <PAT>)`. Docker Hub uses `(<dockerhub-user>, <PAT>)`.
/// For registries we don't recognize, fall back to `("token", <PAT>)`.
fn oci_basic_auth_for(host: &str, token: String) -> (String, String) {
    let user = if host.ends_with("ghcr.io") {
        // GHCR accepts any non-empty username; "USERNAME" is the documented
        // placeholder but the actual GitHub handle also works. Using a static
        // token label keeps the auth deterministic across developers.
        "oauth2".to_string()
    } else {
        "token".to_string()
    };
    (user, token)
}

fn resolve_token(
    home: &Path,
    entry: &greentic_ext_registry::config::RegistryEntry,
) -> Option<String> {
    if let Some(var) = &entry.token_env
        && let Ok(v) = std::env::var(var)
        && !v.is_empty()
    {
        return Some(v);
    }
    let creds = Credentials::load(&home.join("credentials.toml")).ok()?;
    creds.get(&entry.name).map(str::to_string)
}

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
    pub version_override: Option<String>,
    pub trust_policy: String,
    pub verify_only: bool,
    /// Explicit bearer/PAT token for `oci://...` registries. When `None`,
    /// `resolve_backend` falls back to `GHCR_TOKEN` / `GITHUB_TOKEN` /
    /// `OCI_TOKEN` env vars, then anonymous.
    pub oci_token: Option<String>,
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

#[allow(clippy::too_many_lines)]
pub async fn run_publish(cfg: &PublishConfig) -> Result<PublishOutcome, PublishError> {
    // 1. Load + schema-validate describe.json via ext-contract.
    let describe_path = cfg.project_dir.join("describe.json");
    let describe_bytes = std::fs::read(&describe_path).map_err(io_err)?;
    let describe_value: serde_json::Value = serde_json::from_slice(&describe_bytes)
        .map_err(|e| PublishError::DescribeInvalid(format!("parse describe.json: {e}")))?;
    greentic_ext_contract::schema::validate_describe_json(&describe_value)
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
        return verify_only(&backend, &describe, cfg.force);
    }

    // 4. Build (release unless cfg says otherwise).
    let build = run_build(&cfg.project_dir, cfg.profile)
        .map_err(|e| PublishError::Build(format!("cargo component build: {e}")))?;

    // 5. Pack deterministic .gtxpack (staging file).
    let staging_pack = cfg.project_dir.join("dist/publish-staging.gtxpack");
    let info = build_pack(&cfg.project_dir, &build.wasm_path, &staging_pack)
        .map_err(|e| PublishError::Other(anyhow::anyhow!("{e}")))?;
    let pack_bytes = std::fs::read(&staging_pack).map_err(io_err)?;

    // 6. Optional signing (reuse Wave 1 JCS sign_describe).
    let signature = if cfg.sign {
        let key_id = cfg
            .key_id
            .clone()
            .ok_or_else(|| PublishError::Other(anyhow::anyhow!("--sign requires --key-id")))?;
        let signing_key = load_signing_key(&cfg.home, &key_id)
            .map_err(|e| PublishError::Other(anyhow::anyhow!("{e}")))?;
        greentic_ext_contract::sign_describe(&mut describe, &signing_key)
            .map_err(|e| PublishError::Other(anyhow::anyhow!("sign: {e}")))?;
        let sig = describe
            .signature
            .as_ref()
            .ok_or_else(|| PublishError::Other(anyhow::anyhow!("signing produced no signature")))?;
        Some(SignatureBlob {
            algorithm: sig.algorithm.clone(),
            public_key: sig.public_key.clone(),
            value: sig.value.clone(),
            key_id,
        })
    } else {
        None
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

    let receipt = backend.publish(req).await.map_err(map_registry_err)?;

    // 8. Also copy into local ./dist/ with the canonical name.
    let final_dist = cfg.dist_dir.join(format!(
        "{}-{}.gtxpack",
        describe.metadata.name, describe.metadata.version
    ));
    std::fs::create_dir_all(&cfg.dist_dir).map_err(io_err)?;
    std::fs::write(&final_dist, &pack_bytes).map_err(io_err)?;

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

fn verify_only(
    backend: &Backend,
    describe: &DescribeJson,
    force: bool,
) -> Result<PublishOutcome, PublishError> {
    match backend {
        Backend::Local(r) => {
            let ver_dir = r
                .root_path()
                .join(&describe.metadata.id)
                .join(&describe.metadata.version);
            if ver_dir.exists() && !force {
                return Err(PublishError::VersionExists(format!(
                    "version {} already exists at {}",
                    describe.metadata.version,
                    ver_dir.display()
                )));
            }
            Ok(PublishOutcome::VerifyOnly {
                ext_id: describe.metadata.id.clone(),
                version: describe.metadata.version.clone(),
                registry: r.root_path().display().to_string(),
            })
        }
        Backend::Store(r) => {
            // Server-side conflict check lands here in a future iteration;
            // for now, verify_only on a remote registry is a no-op success.
            Ok(PublishOutcome::VerifyOnly {
                ext_id: describe.metadata.id.clone(),
                version: describe.metadata.version.clone(),
                registry: r.base_url().to_string(),
            })
        }
        Backend::Oci(_) => {
            // Server-side HEAD probe against the OCI manifest endpoint lands
            // in a later iteration; for now verify-only is a pass-through.
            Ok(PublishOutcome::VerifyOnly {
                ext_id: describe.metadata.id.clone(),
                version: describe.metadata.version.clone(),
                registry: "oci-registry".into(),
            })
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

fn load_signing_key(home: &Path, key_id: &str) -> anyhow::Result<ed25519_dalek::SigningKey> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let key_path = home.join("keys").join(format!("{key_id}.key"));
    let bytes = std::fs::read_to_string(&key_path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", key_path.display()))?;
    let decoded = B64
        .decode(bytes.trim())
        .map_err(|e| anyhow::anyhow!("decode {key_id}.key: {e}"))?;
    let arr: [u8; 32] = decoded
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("{key_id}.key must be 32 bytes base64"))?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&arr))
}
