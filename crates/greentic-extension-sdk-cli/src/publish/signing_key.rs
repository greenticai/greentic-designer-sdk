//! Signing-key resolution for `gtdx publish --sign` (split out of `mod.rs`).

use std::path::Path;

use super::PublishConfig;

/// True when `key_id` is a single safe path component (no separators, no `..`,
/// non-empty) so it cannot escape `<home>/keys/` when joined.
pub(super) fn is_safe_key_id(key_id: &str) -> bool {
    !key_id.is_empty()
        && !key_id.contains('/')
        && !key_id.contains('\\')
        && key_id != ".."
        && key_id != "."
}

///
/// Key material is a PKCS8 PEM (the one format `gtdx keygen` emits and `gtdx
/// sign` consumes — audit H5). Sources, in precedence order:
/// 1. `--key <path>` — explicit PEM file.
/// 2. `--key-id <id>` — PEM at `<home>/keys/<id>.key`.
/// 3. `--key-env <VAR>` — PEM read from an env var (CI / headless).
///
/// The returned label populates `SignatureBlob.key_id` in the registry
/// metadata: the explicit `--key-id` when given, else a value derived from the
/// key source (file stem, or `"env"`).
pub(super) fn resolve_signing_key(
    cfg: &PublishConfig,
) -> anyhow::Result<(ed25519_dalek::SigningKey, String)> {
    if let Some(path) = &cfg.key_path {
        let key = crate::signing::load_signing_key(Some(path), &cfg.key_env)?;
        let label = cfg.key_id.clone().unwrap_or_else(|| key_id_from_path(path));
        return Ok((key, label));
    }
    if let Some(key_id) = &cfg.key_id {
        if !is_safe_key_id(key_id) {
            anyhow::bail!("invalid --key-id {key_id:?}: must not contain path separators or '..'");
        }
        let key_path = cfg.home.join("keys").join(format!("{key_id}.key"));
        let key = crate::signing::load_signing_key(Some(&key_path), &cfg.key_env)?;
        return Ok((key, key_id.clone()));
    }
    // Fall back to the env var (no file path / key-id supplied).
    let key = crate::signing::load_signing_key(None, &cfg.key_env)?;
    Ok((key, "env".to_string()))
}

/// Derive a `key_id` label from a PEM file path (the file stem, or `"default"`).
fn key_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("default")
        .to_string()
}
