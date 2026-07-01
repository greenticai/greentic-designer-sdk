use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Env override pointing at the `greentic-mcp-gen` binary (absolute path).
pub const MCP_GEN_BIN_ENV: &str = "GTDX_MCP_GEN_BIN";

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

/// Resolve the generator binary: `GTDX_MCP_GEN_BIN` (if it exists) then PATH.
pub fn resolve_mcp_gen() -> anyhow::Result<PathBuf> {
    resolve_mcp_gen_with(non_empty_env(MCP_GEN_BIN_ENV).map(PathBuf::from), || {
        which::which("greentic-mcp-gen").ok()
    })
}

/// Testable core: `override_path` wins if it exists on disk; else `on_path()`.
fn resolve_mcp_gen_with(
    override_path: Option<PathBuf>,
    on_path: impl Fn() -> Option<PathBuf>,
) -> anyhow::Result<PathBuf> {
    if let Some(p) = override_path
        && p.exists()
    {
        return Ok(p);
    }
    if let Some(p) = on_path() {
        return Ok(p);
    }
    anyhow::bail!(
        "greentic-mcp-gen was not found. Install it with \
         `cargo binstall greentic-mcp-generator` (set GITHUB_TOKEN for the private repo), \
         or set {MCP_GEN_BIN_ENV} to its path."
    )
}

/// Artifacts the generator emits into `out_dir` (single-spec path).
pub struct GeneratedArtifacts {
    pub wasm: PathBuf,
    pub meta: Option<PathBuf>,
}

/// Run `greentic-mcp-gen --spec <spec> --output-dir <out_dir>` and locate the
/// newest `*.component.wasm` + its paired `*.component-meta.json`.
pub fn run_generator(
    bin: &Path,
    spec: &Path,
    out_dir: &Path,
) -> anyhow::Result<GeneratedArtifacts> {
    std::fs::create_dir_all(out_dir)?;
    let status = Command::new(bin)
        .arg("--spec")
        .arg(spec)
        .arg("--output-dir")
        .arg(out_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run greentic-mcp-gen: {e}"))?;
    if !status.success() {
        anyhow::bail!("greentic-mcp-gen failed (exit {status})");
    }
    let wasm = newest_matching(out_dir, ".component.wasm")?.ok_or_else(|| {
        anyhow::anyhow!(
            "greentic-mcp-gen produced no *.component.wasm in {}",
            out_dir.display()
        )
    })?;
    // Sidecar is <stem>.component-meta.json paired with <stem>.component.wasm.
    let meta = wasm
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| out_dir.join(n.replace(".component.wasm", ".component-meta.json")))
        .filter(|p| p.is_file());
    Ok(GeneratedArtifacts { wasm, meta })
}

fn newest_matching(dir: &Path, suffix: &str) -> anyhow::Result<Option<PathBuf>> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let is_match = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(suffix));
        if !is_match {
            continue;
        }
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(t, _)| mtime >= *t) {
            best = Some((mtime, path));
        }
    }
    Ok(best.map(|(_, p)| p))
}

/// Patch a rendered mcp `describe.json` with network hosts + secret requirements
/// taken from the generator's `component-meta.json`. Degrades to the rendered
/// values (empty) with a warning when `meta` is absent.
pub fn author_describe_json(rendered: &str, meta: Option<&Path>) -> anyhow::Result<String> {
    let mut doc: serde_json::Value = serde_json::from_str(rendered)
        .map_err(|e| anyhow::anyhow!("rendered describe.json is not valid JSON: {e}"))?;

    let Some(meta_path) = meta else {
        eprintln!(
            "  ! component-meta.json not found — permissions.network and secret_requirements left empty. \
             Update greentic-mcp-generator to auto-fill them, or edit describe.json."
        );
        return serde_json::to_string_pretty(&doc).map_err(Into::into);
    };

    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(meta_path)?)
        .map_err(|e| anyhow::anyhow!("component-meta.json is not valid JSON: {e}"))?;

    // network <= servers (verbatim origins the component calls)
    if let Some(servers) = meta.get("servers").cloned() {
        doc["runtime"]["permissions"]["network"] = servers;
    }
    // secret_requirements <= meta.secret_requirements (same greentic-types shape)
    if let Some(secrets) = meta.get("secret_requirements").cloned() {
        doc["secret_requirements"] = secrets;
    }
    serde_json::to_string_pretty(&doc).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn author_describe_fills_network_and_secrets_from_meta() {
        let rendered = r#"{
  "kind": "wasix:mcp/router",
  "runtime": { "permissions": { "network": [], "secrets": [] } },
  "secret_requirements": []
}"#;
        let dir = tempfile::tempdir().unwrap();
        let meta = dir.path().join("m.json");
        std::fs::write(
            &meta,
            r#"{
  "servers": ["https://api.example.com"],
  "secret_requirements": [{"key":"EXAMPLE_KEY","required":true}],
  "oauth_scopes": []
}"#,
        )
        .unwrap();

        let out = author_describe_json(rendered, Some(&meta)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["runtime"]["permissions"]["network"],
            serde_json::json!(["https://api.example.com"])
        );
        assert_eq!(v["secret_requirements"][0]["key"], "EXAMPLE_KEY");
    }

    #[test]
    fn author_describe_degrades_without_meta() {
        let rendered = r#"{"runtime":{"permissions":{"network":[]}},"secret_requirements":[]}"#;
        let out = author_describe_json(rendered, None).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["runtime"]["permissions"]["network"],
            serde_json::json!([])
        );
        assert_eq!(v["secret_requirements"], serde_json::json!([]));
    }

    #[test]
    fn resolve_mcp_gen_reports_guided_error_when_absent() {
        // Point the override at a non-existent path and ensure a helpful error.
        // SAFETY note: single-threaded test; uses a bogus path so `which` also misses.
        let err = resolve_mcp_gen_with(Some("/nonexistent/greentic-mcp-gen".into()), || None)
            .expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.contains("greentic-mcp-gen"), "guided error: {msg}");
        assert!(
            msg.contains("cargo binstall"),
            "should suggest install: {msg}"
        );
    }

    #[test]
    fn resolve_mcp_gen_prefers_env_override_that_exists() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("greentic-mcp-gen");
        std::fs::write(&bin, b"x").unwrap();
        let got = resolve_mcp_gen_with(Some(bin.clone()), || None).unwrap();
        assert_eq!(got, bin);
    }
}
