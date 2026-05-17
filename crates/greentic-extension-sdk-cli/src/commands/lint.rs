//! `gtdx lint` — pre-publish static checks against `describe.json` that go
//! beyond the JSON Schema validation (`gtdx validate`).
//!
//! Each rule has a stable code (`E_*` for errors, `W_*` for warnings) the
//! test suite asserts on; the human report prefixes every line with the
//! code so authors can grep the docs.
//!
//! Audit-driven rules (May 2026):
//! - `E_VERSION_SEMVER` — `metadata.version` must parse as semver
//! - `E_RUNTIME_REF` — every `nodeTypes[].runtime_ref` and
//!   `tools[].runtime_ref` must name a key declared in `runtime.components`
//! - `E_CAP_CYCLE` — `capabilities.required[].id` must not appear in
//!   `capabilities.offered[].id` (an extension can't depend on a capability
//!   it itself provides — the resolver would happily satisfy via self and
//!   the runtime would then panic on dispatch)
//! - `W_DESCRIBE_DIFF_BREAKING` — the previously-installed describe under
//!   `<home>/extensions/<kind>/<id>/describe.json` exposed contributions or
//!   capabilities that the current describe no longer does, AND
//!   `metadata.version` was not bumped. Warning, not error: prints the
//!   summary and exits zero so it's CI-noise rather than CI-breaking.

use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to an extension source directory containing describe.json.
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

impl Violation {
    fn error(code: &'static str, message: String) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message,
        }
    }

    fn warning(code: &'static str, message: String) -> Self {
        Self {
            code,
            severity: Severity::Warning,
            message,
        }
    }
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{prefix}: {}: {}", self.code, self.message)
    }
}

pub fn run(args: &Args, home: &Path) -> anyhow::Result<()> {
    let describe_path = args.dir.join("describe.json");
    let bytes = std::fs::read(&describe_path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", describe_path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!("parse describe.json: {e}"))?;

    let violations = collect_violations(&value, home);
    for v in &violations {
        eprintln!("{v}");
    }
    let error_count = violations
        .iter()
        .filter(|v| v.severity == Severity::Error)
        .count();
    if error_count == 0 {
        if violations.is_empty() {
            println!("✓ {} lint clean", describe_path.display());
        }
        return Ok(());
    }
    anyhow::bail!("{error_count} error(s)");
}

fn collect_violations(describe: &serde_json::Value, home: &Path) -> Vec<Violation> {
    let mut out = Vec::new();
    out.extend(check_version_semver(describe));
    out.extend(check_runtime_refs(describe));
    out.extend(check_capability_cycle(describe));
    out.extend(check_describe_diff_breaking(describe, home));
    out
}

fn check_version_semver(describe: &serde_json::Value) -> Vec<Violation> {
    let Some(version) = describe
        .pointer("/metadata/version")
        .and_then(|v| v.as_str())
    else {
        return Vec::new();
    };
    if semver::Version::parse(version).is_err() {
        return vec![Violation::error(
            "E_VERSION_SEMVER",
            format!("metadata.version {version:?} is not valid semver"),
        )];
    }
    Vec::new()
}

fn check_runtime_refs(describe: &serde_json::Value) -> Vec<Violation> {
    let declared: std::collections::BTreeSet<&str> = describe
        .pointer("/runtime/components")
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for (group, ptr) in [
        ("nodeTypes", "/contributions/nodeTypes"),
        ("tools", "/contributions/tools"),
    ] {
        let Some(items) = describe.pointer(ptr).and_then(|v| v.as_array()) else {
            continue;
        };
        for (idx, item) in items.iter().enumerate() {
            let Some(rref) = item.get("runtime_ref").and_then(|v| v.as_str()) else {
                continue;
            };
            if !declared.contains(rref) {
                let id = item
                    .get("type_id")
                    .or_else(|| item.get("id"))
                    .and_then(|v| v.as_str())
                    .map_or_else(|| format!("[{idx}]"), str::to_string);
                out.push(Violation::error(
                    "E_RUNTIME_REF",
                    format!(
                        "{group} {id:?} runtime_ref {rref:?} not declared in runtime.components"
                    ),
                ));
            }
        }
    }
    out
}

fn check_capability_cycle(describe: &serde_json::Value) -> Vec<Violation> {
    let offered: std::collections::BTreeSet<String> = describe
        .pointer("/capabilities/offered")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("id").and_then(|i| i.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let required: Vec<&str> = describe
        .pointer("/capabilities/required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("id").and_then(|i| i.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    for req in required {
        if offered.contains(req) {
            out.push(Violation::error(
                "E_CAP_CYCLE",
                format!(
                    "capabilities.required {req:?} is also in capabilities.offered (self-cycle)"
                ),
            ));
        }
    }
    out
}

/// Map a v1/v2 `kind` string (`"DesignExtension"` etc.) to the on-disk
/// directory name (`"design"` etc.) the installer writes into.
fn kind_dir_name(kind: &str) -> Option<&'static str> {
    match kind {
        "DesignExtension" => Some("design"),
        "BundleExtension" => Some("bundle"),
        "DeployExtension" => Some("deploy"),
        "ProviderExtension" => Some("provider"),
        _ => None,
    }
}

/// Pull the set of "named contribution items" (tools, nodeTypes) and
/// offered capability ids out of a describe — the surface authors are
/// most likely to break-by-deletion-without-version-bump.
fn extract_surface(describe: &serde_json::Value) -> Surface {
    let mut s = Surface::default();
    if let Some(arr) = describe
        .pointer("/contributions/tools")
        .and_then(|v| v.as_array())
    {
        for it in arr {
            if let Some(name) = it
                .get("name")
                .or_else(|| it.get("id"))
                .and_then(|v| v.as_str())
            {
                s.tools.insert(name.to_string());
            }
        }
    }
    if let Some(arr) = describe
        .pointer("/contributions/nodeTypes")
        .and_then(|v| v.as_array())
    {
        for it in arr {
            if let Some(id) = it.get("type_id").and_then(|v| v.as_str()) {
                s.node_types.insert(id.to_string());
            }
        }
    }
    if let Some(arr) = describe
        .pointer("/capabilities/offered")
        .and_then(|v| v.as_array())
    {
        for it in arr {
            if let Some(id) = it.get("id").and_then(|v| v.as_str()) {
                s.offered.insert(id.to_string());
            }
        }
    }
    s
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Surface {
    tools: std::collections::BTreeSet<String>,
    node_types: std::collections::BTreeSet<String>,
    offered: std::collections::BTreeSet<String>,
}

fn check_describe_diff_breaking(describe: &serde_json::Value, home: &Path) -> Vec<Violation> {
    let Some(id) = describe.pointer("/metadata/id").and_then(|v| v.as_str()) else {
        return Vec::new();
    };
    let Some(kind) = describe.get("kind").and_then(|v| v.as_str()) else {
        return Vec::new();
    };
    let Some(kind_dir) = kind_dir_name(kind) else {
        return Vec::new();
    };
    let installed_path = home
        .join("extensions")
        .join(kind_dir)
        .join(id)
        .join("describe.json");
    let Ok(prev_bytes) = std::fs::read(&installed_path) else {
        return Vec::new();
    };
    let Ok(prev) = serde_json::from_slice::<serde_json::Value>(&prev_bytes) else {
        return Vec::new();
    };
    let current_version = describe
        .pointer("/metadata/version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let prev_version = prev
        .pointer("/metadata/version")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let current = extract_surface(describe);
    let previous = extract_surface(&prev);

    let removed_tools: Vec<&String> = previous.tools.difference(&current.tools).collect();
    let removed_nodes: Vec<&String> = previous
        .node_types
        .difference(&current.node_types)
        .collect();
    let removed_offered: Vec<&String> = previous.offered.difference(&current.offered).collect();

    if removed_tools.is_empty() && removed_nodes.is_empty() && removed_offered.is_empty() {
        return Vec::new();
    }
    if current_version != prev_version {
        // Version moved — authors signaled the break.
        return Vec::new();
    }

    let mut parts = Vec::new();
    if !removed_tools.is_empty() {
        parts.push(format!("tools removed: {removed_tools:?}"));
    }
    if !removed_nodes.is_empty() {
        parts.push(format!("nodeTypes removed: {removed_nodes:?}"));
    }
    if !removed_offered.is_empty() {
        parts.push(format!("capabilities.offered removed: {removed_offered:?}"));
    }
    vec![Violation::warning(
        "W_DESCRIBE_DIFF_BREAKING",
        format!(
            "breaking change vs installed version {prev_version}: {} (consider bumping metadata.version)",
            parts.join("; ")
        ),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn empty_home() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn version_semver_passes_on_valid() {
        let d = json!({"metadata": {"version": "1.2.3"}});
        assert!(check_version_semver(&d).is_empty());
    }

    #[test]
    fn version_semver_fails_on_invalid() {
        let d = json!({"metadata": {"version": "not-a-version"}});
        let v = check_version_semver(&d);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].code, "E_VERSION_SEMVER");
    }

    #[test]
    fn runtime_ref_passes_when_declared() {
        let d = json!({
            "runtime": {"components": {"ext": {}}},
            "contributions": {"nodeTypes": [{"type_id": "a", "runtime_ref": "ext"}]},
        });
        assert!(check_runtime_refs(&d).is_empty());
    }

    #[test]
    fn runtime_ref_fails_when_dangling() {
        let d = json!({
            "runtime": {"components": {"ext": {}}},
            "contributions": {"nodeTypes": [{"type_id": "a", "runtime_ref": "missing"}]},
        });
        let v = check_runtime_refs(&d);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].code, "E_RUNTIME_REF");
        assert!(v[0].message.contains("missing"));
    }

    #[test]
    fn capability_cycle_passes_when_disjoint() {
        let d = json!({
            "capabilities": {
                "offered": [{"id": "greentic:test/a", "version": "1.0.0"}],
                "required": [{"id": "greentic:test/b", "version": "1.0.0"}],
            },
        });
        assert!(check_capability_cycle(&d).is_empty());
    }

    #[test]
    fn capability_cycle_fails_when_self_required() {
        let d = json!({
            "capabilities": {
                "offered": [{"id": "greentic:test/a", "version": "1.0.0"}],
                "required": [{"id": "greentic:test/a", "version": "1.0.0"}],
            },
        });
        let v = check_capability_cycle(&d);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].code, "E_CAP_CYCLE");
    }

    #[test]
    fn describe_diff_skips_when_no_installed_copy() {
        let home = empty_home();
        let d = json!({
            "kind": "DesignExtension",
            "metadata": {"id": "com.example.x", "version": "0.1.0"},
        });
        assert!(check_describe_diff_breaking(&d, home.path()).is_empty());
    }

    #[test]
    fn describe_diff_skips_when_version_bumped() {
        let home = empty_home();
        let installed_dir = home.path().join("extensions/design/com.example.x");
        std::fs::create_dir_all(&installed_dir).unwrap();
        std::fs::write(
            installed_dir.join("describe.json"),
            serde_json::to_vec(&json!({
                "kind": "DesignExtension",
                "metadata": {"id": "com.example.x", "version": "0.1.0"},
                "contributions": {"tools": [{"name": "a"}, {"name": "b"}]},
            }))
            .unwrap(),
        )
        .unwrap();
        let current = json!({
            "kind": "DesignExtension",
            "metadata": {"id": "com.example.x", "version": "0.2.0"},
            "contributions": {"tools": [{"name": "a"}]},
        });
        assert!(check_describe_diff_breaking(&current, home.path()).is_empty());
    }

    #[test]
    fn describe_diff_warns_when_tool_removed_without_bump() {
        let home = empty_home();
        let installed_dir = home.path().join("extensions/design/com.example.x");
        std::fs::create_dir_all(&installed_dir).unwrap();
        std::fs::write(
            installed_dir.join("describe.json"),
            serde_json::to_vec(&json!({
                "kind": "DesignExtension",
                "metadata": {"id": "com.example.x", "version": "0.1.0"},
                "contributions": {"tools": [{"name": "a"}, {"name": "b"}]},
            }))
            .unwrap(),
        )
        .unwrap();
        let current = json!({
            "kind": "DesignExtension",
            "metadata": {"id": "com.example.x", "version": "0.1.0"},
            "contributions": {"tools": [{"name": "a"}]},
        });
        let v = check_describe_diff_breaking(&current, home.path());
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].code, "W_DESCRIBE_DIFF_BREAKING");
        assert_eq!(v[0].severity, Severity::Warning);
        assert!(v[0].message.contains("\"b\""), "msg: {}", v[0].message);
    }

    #[test]
    fn describe_diff_warns_when_capability_offered_removed_without_bump() {
        let home = empty_home();
        let installed_dir = home.path().join("extensions/design/com.example.y");
        std::fs::create_dir_all(&installed_dir).unwrap();
        std::fs::write(
            installed_dir.join("describe.json"),
            serde_json::to_vec(&json!({
                "kind": "DesignExtension",
                "metadata": {"id": "com.example.y", "version": "0.1.0"},
                "capabilities": {
                    "offered": [{"id": "greentic:test/a", "version": "1.0.0"}],
                    "required": [],
                },
            }))
            .unwrap(),
        )
        .unwrap();
        let current = json!({
            "kind": "DesignExtension",
            "metadata": {"id": "com.example.y", "version": "0.1.0"},
            "capabilities": {"offered": [], "required": []},
        });
        let v = check_describe_diff_breaking(&current, home.path());
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].code, "W_DESCRIBE_DIFF_BREAKING");
        assert!(
            v[0].message.contains("capabilities.offered removed"),
            "msg: {}",
            v[0].message
        );
    }
}
