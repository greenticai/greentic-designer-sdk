//! Lint rule implementations for `gtdx lint` (split out of `mod.rs` to keep
//! source files under the 500-line limit). Each rule returns the violations it
//! finds; `mod.rs` owns orchestration and reporting.

use std::path::Path;

use super::Violation;

pub(super) fn check_version_semver(describe: &serde_json::Value) -> Vec<Violation> {
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

pub(super) fn check_runtime_refs(describe: &serde_json::Value) -> Vec<Violation> {
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

pub(super) fn check_capability_cycle(describe: &serde_json::Value) -> Vec<Violation> {
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

/// True when `current` is a semver bump over `prev` that signals a breaking
/// change: a major bump (for `>=1.0`), or a minor bump while in `0.x` (where
/// Cargo semver treats minor as breaking). Downgrades and equal versions are
/// never breaking bumps.
pub(super) fn is_breaking_bump(prev: &semver::Version, current: &semver::Version) -> bool {
    if current <= prev {
        return false;
    }
    if current.major > prev.major {
        return true;
    }
    prev.major == 0 && current.minor > prev.minor
}

pub(super) fn check_describe_diff_breaking(
    describe: &serde_json::Value,
    home: &Path,
) -> Vec<Violation> {
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
    // Only suppress when the version moved in a way that actually *signals* a
    // breaking change. A downgrade, an equal version, or a mere patch/minor
    // bump (for >=1.0) does not — those still warrant the warning.
    if let (Ok(prev_v), Ok(cur_v)) = (
        semver::Version::parse(prev_version),
        semver::Version::parse(current_version),
    ) && is_breaking_bump(&prev_v, &cur_v)
    {
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
