use greentic_extension_sdk_contract::DescribeJson;
use greentic_extension_sdk_contract::migration::migrate_v0_4_x_value;

const V1_AC: &str = include_str!("fixtures/v1_ac.json");
const V1_LLM: &str = include_str!("fixtures/v1_llm_openai.json");

/// A v1 signature was computed over v1 canonical bytes; after migration the
/// canonical form is different, so the carried-over signature would be
/// misleading. The migrator must DROP it and add a warning (audit L2).
#[test]
fn migration_strips_stale_signature_and_warns() {
    let mut v1: serde_json::Value = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "DesignExtension",
        "metadata": {
            "id": "greentic.test-sig",
            "name": "Test Sig",
            "version": "1.0.0",
            "summary": "Test fixture for signature stripping",
            "author": { "name": "Greentic" },
            "license": "MIT"
        },
        "engine": { "greenticDesigner": ">=1.2.0", "extRuntime": "^1.2.0" },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "component": "extension.wasm",
            "memoryLimitMB": 32,
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
        },
        "contributions": {}
    });

    // Inject a stale v1 signature to simulate a signed v1 descriptor.
    v1.as_object_mut().unwrap().insert(
        "signature".into(),
        serde_json::json!({
            "algorithm": "ed25519",
            "value": "deadbeefdeadbeef"
        }),
    );

    let (out, report) = migrate_v0_4_x_value(&v1).expect("migrate should succeed");

    assert!(
        out.get("signature").is_none(),
        "stale v1 signature must be dropped from migrated output; got: {:?}",
        out.get("signature")
    );
    assert!(
        report.warnings.iter().any(|w| w.contains("signature")),
        "must warn that re-signing is required; warnings were: {:?}",
        report.warnings
    );
}

#[test]
fn ac_v1_migrates_to_parseable_v2() {
    let raw: serde_json::Value = serde_json::from_str(V1_AC).unwrap();
    let (v2, report) = migrate_v0_4_x_value(&raw).unwrap();
    let d: DescribeJson = serde_json::from_value(v2.clone()).expect("v2 parses");
    assert_eq!(d.api_version, "greentic.ai/v2");
    assert!(!d.runtime.components.is_empty());
    // AC has no gtpack in v1 — migration carries the `runtime.component`
    // path through with a placeholder sha and warns it must be re-hashed
    // before publishing (audit P0-2 / P1-8).
    assert!(
        report.warnings.iter().any(|w| w.contains("sha256")),
        "expected placeholder-sha warning, got {:?}",
        report.warnings
    );
}

/// Audit P0-2: a v1 describe whose only artifact reference is
/// `runtime.component` (the WASM path) must NOT lose that path during
/// migration. The previous behaviour emitted a `placeholder://zero`
/// `oci_ref` and dropped the real path entirely, turning every gtpack-less
/// v1 extension into an un-publishable placeholder.
#[test]
fn migration_carries_runtime_component_path() {
    let v1 = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "DesignExtension",
        "metadata": {
            "id": "greentic.carry-test",
            "name": "Carry Test",
            "version": "1.0.0",
            "summary": "Fixture for runtime.component carry-through",
            "author": { "name": "Greentic" },
            "license": "MIT"
        },
        "engine": { "greenticDesigner": ">=1.2.0", "extRuntime": "^1.2.0" },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "component": "build/my-extension.wasm",
            "memoryLimitMB": 32,
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
        },
        "contributions": {}
    });

    let (v2, _report) = migrate_v0_4_x_value(&v1).expect("migrate should succeed");
    let d: DescribeJson = serde_json::from_value(v2).expect("v2 parses");
    let comp = d
        .runtime
        .components
        .values()
        .next()
        .expect("at least one component");

    // The real WASM path must survive — not a `placeholder://zero` oci_ref.
    assert_ne!(
        comp.oci_ref.as_deref(),
        Some("placeholder://zero"),
        "migration dropped runtime.component and emitted a placeholder oci_ref"
    );
    let gtpack = comp
        .gtpack
        .as_ref()
        .expect("runtime.component must be carried into a gtpack entry");
    assert_eq!(
        gtpack.file, "build/my-extension.wasm",
        "the v1 runtime.component path must be preserved in gtpack.file"
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
