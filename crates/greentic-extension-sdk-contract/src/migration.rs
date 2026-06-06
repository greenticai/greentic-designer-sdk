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

    out.insert(
        "compat".into(),
        json!({
            "min_designer_version": ">=1.2.0",
            "min_runner_version": "^0.12.0",
            "contract_version": "1.2.0"
        }),
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

fn migrate_contributions(obj: &Map<String, Value>, report: &mut MigrationReport) -> Value {
    let empty = Value::Object(Map::new());
    let contributions_in = obj.get("contributions").unwrap_or(&empty);
    let mut out = Map::new();

    if let Some(arr) = contributions_in.get("nodeTypes").and_then(Value::as_array) {
        // LocalizedString handles plain strings transparently; pass through as-is.
        out.insert("nodeTypes".into(), Value::Array(arr.clone()));
    }
    for key in ["tools", "recipes"] {
        if let Some(v) = contributions_in.get(key).cloned() {
            out.insert(key.into(), v);
        }
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
