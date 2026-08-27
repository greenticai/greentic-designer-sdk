//! Lint rule implementations for `gtdx lint` (split out of `mod.rs` to keep
//! source files under the 500-line limit). Each rule returns the violations it
//! finds; `mod.rs` owns orchestration and reporting.

use std::path::Path;

use greentic_extension_sdk_contract::extension_id::validate_extension_id;

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

// ---------------------------------------------------------------------------
// Governance rules (2026-06) — see docs/superpowers/specs governance design.
// ---------------------------------------------------------------------------

/// The single canonical `$schema` / `$id` URL every describe.json must use.
pub(super) const CANONICAL_SCHEMA_URL: &str =
    "https://store.greentic.cloud/schemas/describe-v2.json";

/// Canonical namespace prefix for a fully-qualified design-extension export.
/// A tool's `export` must be `greentic:extension-design/<interface>.<member>`
/// (e.g. `tools.invoke-tool`, `validation.validate-content`,
/// `knowledge.get-entry`) — never a bare member name like `invoke-tool`.
pub(super) const EXPORT_NAMESPACE: &str = "greentic:extension-design/";

/// `E_SCHEMA_HOST` — `$schema` must equal the canonical URL exactly.
pub(super) fn check_schema_host(describe: &serde_json::Value) -> Vec<Violation> {
    let Some(schema) = describe.get("$schema").and_then(|v| v.as_str()) else {
        return vec![Violation::error(
            "E_SCHEMA_HOST",
            format!("$schema is missing; must be {CANONICAL_SCHEMA_URL}"),
        )];
    };
    if schema != CANONICAL_SCHEMA_URL {
        return vec![Violation::error(
            "E_SCHEMA_HOST",
            format!("$schema must be {CANONICAL_SCHEMA_URL} (found {schema:?})"),
        )];
    }
    Vec::new()
}

/// `E_EXPORT_FORM` — every tool's `export` must be a fully-qualified
/// `greentic:extension-design/<interface>.<member>` reference, not a bare
/// member name (e.g. `invoke-tool`). Different interfaces are allowed
/// (`tools`, `validation`, `knowledge`, …) — only the namespace form is fixed.
pub(super) fn check_export_form(describe: &serde_json::Value) -> Vec<Violation> {
    let Some(tools) = describe
        .pointer("/contributions/tools")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (idx, tool) in tools.iter().enumerate() {
        let Some(export) = tool.get("export").and_then(|v| v.as_str()) else {
            continue;
        };
        let qualified = export
            .strip_prefix(EXPORT_NAMESPACE)
            .is_some_and(|member| member.contains('.'));
        if !qualified {
            out.push(Violation::error(
                "E_EXPORT_FORM",
                format!(
                    "tools[{idx}].export must be a fully-qualified \
                     {EXPORT_NAMESPACE}<interface>.<member> reference (found {export:?})"
                ),
            ));
        }
    }
    out
}

/// `E_ENGINE_DEPRECATED` — the `engine` block is forbidden; compat is the sole
/// source of version constraints.
pub(super) fn check_engine_deprecated(describe: &serde_json::Value) -> Vec<Violation> {
    if describe.get("engine").is_some() {
        return vec![Violation::error(
            "E_ENGINE_DEPRECATED",
            "engine block is deprecated; move all version constraints into compat \
             (min_designer_version / min_runner_version) and delete engine"
                .to_string(),
        )];
    }
    Vec::new()
}

/// True when a hex string is present and entirely zeros (the placeholder).
fn is_zero_hash(value: &serde_json::Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| !s.is_empty() && s.chars().all(|c| c == '0'))
}

/// `E_SHA256_ZERO` — (publish only) no `sha256` under `runtime.components` may
/// be a placeholder of all zeros.
pub(super) fn check_sha256_zero(describe: &serde_json::Value, publish: bool) -> Vec<Violation> {
    if !publish {
        return Vec::new();
    }
    let Some(components) = describe
        .pointer("/runtime/components")
        .and_then(|v| v.as_object())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, component) in components {
        if let Some(sha) = component.get("sha256")
            && is_zero_hash(sha)
        {
            out.push(Violation::error(
                "E_SHA256_ZERO",
                format!("runtime.components.{name}.sha256 is a placeholder (all zeros)"),
            ));
        }
        if let Some(sha) = component.pointer("/gtpack/sha256")
            && is_zero_hash(sha)
        {
            out.push(Violation::error(
                "E_SHA256_ZERO",
                format!("runtime.components.{name}.gtpack.sha256 is a placeholder (all zeros)"),
            ));
        }
    }
    out
}

/// `E_ID_PATTERN` — `metadata.id` must be reverse-DNS.
///
/// The rule lives in `greentic_extension_sdk_contract::extension_id`, shared
/// with `gtdx new` and `gtdx publish`, so all three agree and quote the same
/// pattern. It no longer requires the `greentic.` namespace: any reverse-DNS
/// namespace the author controls is valid, matching `describe-v2.json`.
pub(super) fn check_id_pattern(describe: &serde_json::Value) -> Vec<Violation> {
    let Some(id) = describe.pointer("/metadata/id").and_then(|v| v.as_str()) else {
        return Vec::new();
    };
    match validate_extension_id(id) {
        Ok(()) => Vec::new(),
        Err(e) => vec![Violation::error("E_ID_PATTERN", e.to_string())],
    }
}

/// `snake_case`: starts with a lowercase letter, then lowercase / digits / `_`.
fn is_snake_case(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// True when `short` is a prefix of `long` ending on a `_` boundary, e.g.
/// `generate_gtpack` vs `generate_gtpack_from_sorla_yaml`.
fn is_prefix_on_boundary(short: &str, long: &str) -> bool {
    long.len() > short.len() && long.starts_with(short) && long.as_bytes()[short.len()] == b'_'
}

/// `E_TOOL_NAMING` — tool names must be `snake_case` and free of near-duplicate
/// prefix pairs.
pub(super) fn check_tool_naming(describe: &serde_json::Value) -> Vec<Violation> {
    let Some(tools) = describe
        .pointer("/contributions/tools")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
        .collect();

    let mut out = Vec::new();
    for name in &names {
        if !is_snake_case(name) {
            out.push(Violation::error(
                "E_TOOL_NAMING",
                format!("tool name {name:?} is not snake_case"),
            ));
        }
    }
    for (i, a) in names.iter().enumerate() {
        for b in names.iter().skip(i + 1) {
            let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
            if is_prefix_on_boundary(short, long) {
                out.push(Violation::error(
                    "E_TOOL_NAMING",
                    format!(
                        "tool names {short:?} and {long:?} are near-duplicates (prefix overlap)"
                    ),
                ));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Secret-permission hygiene (S4)
// ---------------------------------------------------------------------------

/// Returns `true` when `entry` looks like a **grant** (a read-permission token)
/// rather than a plain field-name key.
///
/// A grant is one of:
/// - a URI containing `"://"` (e.g. `"secret://tavily/"`)
/// - a bare wildcard `"*"`
/// - a path prefix ending with `"/"` (e.g. `"tavily/"`)
///
/// Anything else is treated as a plain field-name key that belongs in
/// `requiredSecrets`, not in `runtime.permissions.secrets`.
fn looks_like_grant(entry: &str) -> bool {
    entry == "*" || entry.contains("://") || entry.ends_with('/')
}

/// `E_PERMS_SECRETS_PLAIN_KEY` — errors when `runtime.permissions.secrets`
/// contains a plain field-name key (e.g. `"SLACK_BOT_TOKEN"`, `"api_key"`).
///
/// The `permissions.secrets` array is for **read-permission grants** only
/// (URI prefixes like `"secret://tavily/"`, or `"*"` for wildcard access).
/// Credential field names that an operator must supply belong in the top-level
/// `requiredSecrets` array instead. See `docs/authoring-secrets.md`.
pub(super) fn check_perms_secrets_plain_key(describe: &serde_json::Value) -> Vec<Violation> {
    let Some(entries) = describe
        .pointer("/runtime/permissions/secrets")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|s| !looks_like_grant(s))
        .map(|key| {
            Violation::error(
                "E_PERMS_SECRETS_PLAIN_KEY",
                format!(
                    "runtime.permissions.secrets contains plain key {key:?}; \
                     credential field names belong in the top-level `requiredSecrets` array, \
                     not in permissions.secrets (which is for read-permission grants only, \
                     e.g. \"secret://tavily/\" or \"*\")"
                ),
            )
        })
        .collect()
}
