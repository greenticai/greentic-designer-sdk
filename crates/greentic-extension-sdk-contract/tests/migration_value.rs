use greentic_extension_sdk_contract::DescribeJson;
use greentic_extension_sdk_contract::migration::migrate_v0_4_x_value;

const V1_AC: &str = include_str!("fixtures/v1_ac.json");
const V1_LLM: &str = include_str!("fixtures/v1_llm_openai.json");

#[test]
fn ac_v1_migrates_to_parseable_v2() {
    let raw: serde_json::Value = serde_json::from_str(V1_AC).unwrap();
    let (v2, report) = migrate_v0_4_x_value(&raw).unwrap();
    let d: DescribeJson = serde_json::from_value(v2.clone()).expect("v2 parses");
    assert_eq!(d.api_version, "greentic.ai/v2");
    assert!(!d.runtime.components.is_empty());
    // AC has no gtpack in v1 — migration emits a zero-sha warning.
    assert!(
        report.warnings.iter().any(|w| w.contains("zero sha256")),
        "expected zero-sha warning, got {:?}",
        report.warnings
    );
}

#[test]
fn llm_openai_v1_migrates_carrying_gtpack() {
    let raw: serde_json::Value = serde_json::from_str(V1_LLM).unwrap();
    let (v2, _report) = migrate_v0_4_x_value(&raw).unwrap();
    let d: DescribeJson = serde_json::from_value(v2).unwrap();
    let comp = d.runtime.components.values().next().unwrap();
    assert!(comp.gtpack.is_some());
    let g = comp.gtpack.as_ref().unwrap();
    assert_eq!(g.pack_id, "greentic.llm-openai");
}

#[test]
fn deploy_targets_dropped_with_warning() {
    let raw = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "DeployExtension",
        "metadata": {
            "id": "greentic.deploy-test",
            "name": "T",
            "version": "1.0.0",
            "summary": "T",
            "author": { "name": "Greentic" },
            "license": "MIT"
        },
        "engine": { "greenticDesigner": "*", "extRuntime": "^0.1.0" },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "component": "extension.wasm",
            "memoryLimitMB": 32,
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
        },
        "contributions": {
            "targets": [{ "id": "x", "displayName": "X", "supportsRollback": true }]
        }
    });
    let (_v2, report) = migrate_v0_4_x_value(&raw).unwrap();
    assert!(report.dropped_keys.contains(&"targets".to_string()));
}
