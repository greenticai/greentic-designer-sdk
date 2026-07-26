//! v1 → v2 migration helpers. Consumer extensions invoke `migrate_v0_4_x_value`
//! during build.sh to translate their describe.json forward; gtdx-cli will
//! call the same helper in `gtdx migrate` (Phase E).

use serde_json::{Map, Value, json};

use crate::error::ContractError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub warnings: Vec<String>,
    pub dropped_keys: Vec<String>,
}

impl MigrationReport {
    pub fn warn<S: Into<String>>(&mut self, msg: S) {
        self.warnings.push(msg.into());
    }

    pub fn dropped<S: Into<String>>(&mut self, key: S) {
        self.dropped_keys.push(key.into());
    }
}

/// Convert a v1 describe.json (`apiVersion: greentic.ai/v1`) into a v2 value
/// suitable for `serde_json::from_value::<DescribeJson>()`.
///
/// Returns the converted value + a non-fatal report. Hard errors return `Err`.
pub fn migrate_v0_4_x_value(raw: &Value) -> Result<(Value, MigrationReport), ContractError> {
    let mut report = MigrationReport::default();
    let obj = raw
        .as_object()
        .ok_or_else(|| ContractError::SchemaInvalid("describe must be object".into()))?;

    let api = obj.get("apiVersion").and_then(Value::as_str).unwrap_or("");
    // Idempotent: an already-v2 document migrates to itself, so running
    // `gtdx migrate` twice is a no-op rather than an error (audit cycle-2 P3).
    if api == "greentic.ai/v2" {
        return Ok((raw.clone(), MigrationReport::default()));
    }
    if api != "greentic.ai/v1" {
        return Err(ContractError::UnsupportedApiVersion(api.into()));
    }

    let mut out = Map::new();
    out.insert("apiVersion".into(), Value::String("greentic.ai/v2".into()));
    out.insert(
        "$schema".into(),
        Value::String("https://store.greentic.cloud/schemas/describe-v2.json".into()),
    );

    for key in ["kind", "metadata", "engine", "capabilities"] {
        if let Some(v) = obj.get(key).cloned() {
            out.insert(key.into(), v);
        }
    }

    // v1 has no `compat` block, so it is synthesized with conservative defaults.
    // Carry the v1 `engine.greenticDesigner` constraint into `min_designer_version`
    // when present (the other axes have no v1 equivalent), and warn that the
    // block was fabricated so the author reviews it before publishing (audit P3).
    // Only carry the v1 constraint forward if it is a valid `VersionReq`; a junk
    // value falls back to the default rather than introducing a new migration
    // parse-failure that the old hardcoded constant never had.
    let min_designer = obj
        .get("engine")
        .and_then(|e| e.get("greenticDesigner"))
        .and_then(Value::as_str)
        .filter(|s| s.parse::<semver::VersionReq>().is_ok())
        .unwrap_or(">=1.2.0");
    out.insert(
        "compat".into(),
        json!({
            "min_designer_version": min_designer,
            "min_runner_version": "^0.12.0",
            "contract_version": "1.2.0"
        }),
    );
    report.warn(
        "synthesized a `compat` block (v1 has none); review min_designer/min_runner/contract_version \
         before publishing",
    );

    let (runtime_val, contributions_val) = migrate_runtime_and_contributions(obj, &mut report);
    out.insert("runtime".into(), runtime_val);
    out.insert("contributions".into(), contributions_val);

    // A v1 signature was computed over v1 canonical bytes; after migration the
    // canonical form differs entirely, so carrying it would be misleading.
    // Drop it and require re-signing (audit L2).
    if obj.get("signature").is_some() {
        report.warn("dropped v1 signature — migrated descriptor must be re-signed");
    }

    Ok((Value::Object(out), report))
}

/// Build the v2 `runtime` object and the v2 `contributions` object.
fn migrate_runtime_and_contributions(
    obj: &Map<String, Value>,
    report: &mut MigrationReport,
) -> (Value, Value) {
    let runtime_val = migrate_runtime(obj, report);
    let contributions_val = migrate_contributions(obj, report);
    (runtime_val, contributions_val)
}

fn migrate_runtime(obj: &Map<String, Value>, report: &mut MigrationReport) -> Value {
    let mut runtime_out = Map::new();
    if let Some(rt) = obj.get("runtime").and_then(Value::as_object) {
        for key in ["memoryLimitMB", "permissions"] {
            if let Some(v) = rt.get(key).cloned() {
                runtime_out.insert(key.into(), v);
            }
        }
        let entry = build_component_entry(rt, report);
        let mut components = Map::new();
        components.insert("main".into(), entry);
        runtime_out.insert("components".into(), Value::Object(components));
    }
    Value::Object(runtime_out)
}

fn build_component_entry(rt: &Map<String, Value>, report: &mut MigrationReport) -> Value {
    let mut entry = Map::new();
    if let Some(gtpack) = rt.get("gtpack").cloned() {
        let sha = gtpack
            .get("sha256")
            .and_then(Value::as_str)
            .unwrap_or(&"0".repeat(64))
            .to_string();
        entry.insert("gtpack".into(), gtpack);
        entry.insert("sha256".into(), Value::String(sha));
        entry.insert("world".into(), Value::String("main".into()));
    } else if let Some(component_path) = rt.get("component").and_then(Value::as_str) {
        // No gtpack, but the v1 doc carries `runtime.component` — the WASM
        // artifact path (e.g. "extension.wasm"). Carry it into a gtpack entry
        // so the only component reference the extension has is preserved.
        // The sha256 is not known at migration time (the v1 schema doesn't
        // record it), so emit a placeholder and warn it must be re-hashed
        // before publishing (audit P0-2 / P1-8).
        let zero_sha = "0".repeat(64);
        entry.insert(
            "gtpack".into(),
            json!({
                "file": component_path,
                "sha256": zero_sha,
                "pack_id": "main",
                "component_version": "0.0.0"
            }),
        );
        entry.insert("sha256".into(), Value::String(zero_sha.clone()));
        entry.insert("world".into(), Value::String("main".into()));
        report.warn(format!(
            "runtime.components[\"main\"] carried v1 runtime.component path \
             \"{component_path}\" into gtpack with a zero sha256; re-hash the \
             artifact before publishing"
        ));
    } else {
        // Neither gtpack nor runtime.component — nothing to point at. Emit a
        // placeholder oci_ref so RuntimeComponent deserialises (it requires at
        // least one of oci_ref or gtpack) and warn loudly.
        entry.insert("oci_ref".into(), Value::String("placeholder://zero".into()));
        entry.insert("sha256".into(), Value::String("0".repeat(64)));
        entry.insert("world".into(), Value::String("main".into()));
        report.warn(
            "runtime.components[\"main\"] populated with zero sha256 \
             and placeholder oci_ref; please hand-edit before publishing",
        );
    }
    Value::Object(entry)
}

/// Normalize a v1 node-type/recipe entry's `config_schema` to the single shape
/// the v2 contract requires: a `String` under the ``snake_case`` key.
///
/// v1 authored it three inconsistent ways, all seen in the real bundled packs:
/// an inline JSON **object** (http/webhook/llm-generic), the ``camelCase`` key
/// **`configSchema`** (bundle-standard recipes), or **`null`**
/// (platform-bootstrap). An object is serialized back to a JSON string; a null
/// or missing schema becomes an empty object string; a string is kept as-is.
fn normalize_config_schema(item: &Value) -> Value {
    let Some(map) = item.as_object() else {
        return item.clone();
    };
    let mut map = map.clone();

    // Fold camelCase `configSchema` into snake_case `config_schema` (only when
    // the `snake_case` key isn't already present, so an author who wrote both
    // does not get the camelCase one silently win).
    if let Some(v) = map.remove("configSchema") {
        map.entry("config_schema").or_insert(v);
    }

    let normalized = match map.get("config_schema") {
        Some(Value::String(s)) => Value::String(s.clone()),
        Some(Value::Object(o)) => {
            Value::String(serde_json::to_string(&Value::Object(o.clone())).unwrap_or_default())
        }
        // `null`, or any other non-string shape, and the absent case all
        // collapse to an empty JSON object — a valid, no-op schema string.
        _ => Value::String("{}".into()),
    };
    map.insert("config_schema".into(), normalized);
    Value::Object(map)
}

/// Lower a `camelCase` key to `snake_case` (`displayName` → `display_name`), used
/// to reconcile v1's `camelCase` contribution keys with the v2 structs'
/// `snake_case` (`rename_all`) fields.
fn camel_to_snake(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 2);
    for ch in key.chars() {
        if ch.is_ascii_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Normalize a v1 recipe to the v2 `Recipe` shape: `snake_case` keys, only the
/// fields v2 knows (`deny_unknown_fields` rejects the rest), and a
/// string-valued `config_schema`.
fn normalize_recipe(item: &Value) -> Value {
    const KNOWN: [&str; 4] = ["id", "display_name", "description", "config_schema"];
    let Some(map) = item.as_object() else {
        return item.clone();
    };
    let mut out = Map::new();
    for (key, value) in map {
        let snake = camel_to_snake(key);
        if KNOWN.contains(&snake.as_str()) {
            out.insert(snake, value.clone());
        }
        // Unknown v1 fields (e.g. `supportedCapabilities`) have no v2 slot and
        // are dropped.
    }
    normalize_config_schema(&Value::Object(out))
}

fn migrate_contributions(obj: &Map<String, Value>, report: &mut MigrationReport) -> Value {
    let empty = Value::Object(Map::new());
    let contributions_in = obj.get("contributions").unwrap_or(&empty);
    let mut out = Map::new();

    if let Some(arr) = contributions_in.get("nodeTypes").and_then(Value::as_array) {
        // LocalizedString handles plain strings transparently. But v1 authored
        // `config_schema` inconsistently — as an inline JSON object, as `null`,
        // or under the `camelCase` key `configSchema` — while the v2 `NodeType`
        // requires a single `config_schema: String`. Normalize each entry.
        let normalized = arr.iter().map(normalize_config_schema).collect();
        out.insert("nodeTypes".into(), Value::Array(normalized));
    }
    if let Some(v) = contributions_in.get("tools").cloned() {
        out.insert("tools".into(), v);
    }
    if let Some(arr) = contributions_in.get("recipes").and_then(Value::as_array) {
        // Recipes drifted further than node types: v1 authored their keys in
        // camelCase (`displayName`, `configSchema`) and carried extra fields
        // (`supportedCapabilities`) the v2 `Recipe` (deny_unknown_fields) has no
        // slot for. Normalize keys to snake_case, keep only known fields, and
        // fix `config_schema`.
        let normalized = arr.iter().map(normalize_recipe).collect();
        out.insert("recipes".into(), Value::Array(normalized));
    } else if let Some(v) = contributions_in.get("recipes").cloned() {
        out.insert("recipes".into(), v);
    }
    // knowledge / prompts / schemas: v1 allows plain path strings; v2 requires {path}.
    for key in ["knowledge", "prompts", "schemas"] {
        if let Some(arr) = contributions_in.get(key).and_then(Value::as_array) {
            let wrapped = arr
                .iter()
                .map(|v| {
                    if v.is_string() {
                        json!({ "path": v })
                    } else {
                        v.clone()
                    }
                })
                .collect();
            out.insert(key.into(), Value::Array(wrapped));
        }
    }

    // `targets` is a DeployExtension-only concept with no v2 equivalent, so it
    // is dropped outright. It must NOT be turned into a top-level `execution`
    // block: `execution` is only valid for kind=BundleExtension, and `targets`
    // only ever appears on a DeployExtension, so synthesizing `execution` here
    // produces a v2 document that fails `DescribeJson` validation (audit N1).
    if contributions_in.get("targets").is_some() {
        report.dropped("targets");
    }

    Value::Object(out)
}
