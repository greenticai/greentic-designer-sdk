//! `gtdx lint` — pre-publish static checks against `describe.json` that go
//! beyond the JSON Schema validation (`gtdx validate`).
//!
//! Each rule has a stable code (`E_*`) the test suite asserts on; the human
//! report prefixes every line with the code so authors can grep the docs.
//!
//! Audit-driven rules (May 2026):
//! - `E_VERSION_SEMVER` — `metadata.version` must parse as semver
//! - `E_RUNTIME_REF` — every `nodeTypes[].runtime_ref` and
//!   `tools[].runtime_ref` must name a key declared in `runtime.components`
//! - `E_CAP_CYCLE` — `capabilities.required[].id` must not appear in
//!   `capabilities.offered[].id` (an extension can't depend on a capability
//!   it itself provides — the resolver would happily satisfy via self and
//!   the runtime would then panic on dispatch)

use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to an extension source directory containing describe.json.
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

pub fn run(args: &Args, _home: &Path) -> anyhow::Result<()> {
    let describe_path = args.dir.join("describe.json");
    let bytes = std::fs::read(&describe_path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", describe_path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!("parse describe.json: {e}"))?;

    let violations = collect_violations(&value);
    if violations.is_empty() {
        println!("✓ {} lint clean", describe_path.display());
        return Ok(());
    }
    for v in &violations {
        eprintln!("{v}");
    }
    anyhow::bail!("{} violation(s)", violations.len());
}

fn collect_violations(describe: &serde_json::Value) -> Vec<Violation> {
    let mut out = Vec::new();
    out.extend(check_version_semver(describe));
    out.extend(check_runtime_refs(describe));
    out.extend(check_capability_cycle(describe));
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
        return vec![Violation {
            code: "E_VERSION_SEMVER",
            message: format!("metadata.version {version:?} is not valid semver"),
        }];
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
                out.push(Violation {
                    code: "E_RUNTIME_REF",
                    message: format!(
                        "{group} {id:?} runtime_ref {rref:?} not declared in runtime.components"
                    ),
                });
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
            out.push(Violation {
                code: "E_CAP_CYCLE",
                message: format!(
                    "capabilities.required {req:?} is also in capabilities.offered (self-cycle)"
                ),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
