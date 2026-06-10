//! Registry backend resolution for `gtdx publish`: local filesystem,
//! Greentic store, or OCI. Split out of `mod.rs` to keep source files under
//! the 500-line limit.

use std::path::Path;

use greentic_extension_sdk_registry::credentials::Credentials;
use greentic_extension_sdk_registry::local::LocalFilesystemRegistry;
use greentic_extension_sdk_registry::oci::OciRegistry;
use greentic_extension_sdk_registry::registry::ExtensionRegistry;
use greentic_extension_sdk_registry::store::GreenticStoreRegistry;

pub(super) enum Backend {
    Local(LocalFilesystemRegistry),
    Store(GreenticStoreRegistry),
    Oci(OciRegistry),
}

impl Backend {
    pub(super) async fn publish(
        &self,
        req: greentic_extension_sdk_registry::publish::PublishRequest,
    ) -> Result<
        greentic_extension_sdk_registry::publish::PublishReceipt,
        greentic_extension_sdk_registry::RegistryError,
    > {
        match self {
            Backend::Local(r) => r.publish(req).await,
            Backend::Store(r) => r.publish(req).await,
            Backend::Oci(r) => r.publish(req).await,
        }
    }
}

pub(super) fn resolve_backend(
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

    let cfg = greentic_extension_sdk_registry::config::load(&home.join("config.toml"))
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
    let allow_insecure = crate::registry_security::insecure_registry_opt_in();
    Ok(Backend::Store(
        GreenticStoreRegistry::new(&entry.name, &entry.url, token)
            .with_insecure_allowed(allow_insecure),
    ))
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
    entry: &greentic_extension_sdk_registry::config::RegistryEntry,
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
