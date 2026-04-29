//! Regression tests for the `DesignExtension + runtime.gtpack + nodeTypes` invariant.

use greentic_extension_sdk_contract::DescribeJson;

/// Build a valid describe.json skeleton for the given kind.
///
/// Kind must be one of `DesignExtension` / `ProviderExtension` / `BundleExtension` / `DeployExtension`.
/// Returns a `serde_json::Value` ready to be customised by the caller before deserialising.
fn skeleton(kind: &str) -> serde_json::Value {
    serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": kind,
        "metadata": {
            "id": "test.ext",
            "name": "Test",
            "version": "0.1.0",
            "summary": "fixture",
            "author": { "name": "t" },
            "license": "MIT"
        },
        "engine": { "greenticDesigner": "*", "extRuntime": "^0.1.0" },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "component": "extension.wasm",
            "memoryLimitMB": 32,
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
        },
        "contributions": {}
    })
}

fn with_gtpack(mut v: serde_json::Value) -> serde_json::Value {
    v["runtime"]["gtpack"] = serde_json::json!({
        "file": "runtime/x.gtpack",
        "sha256": "a".repeat(64),
        "pack_id": "test.pack",
        "component_version": "0.6.0"
    });
    v
}

fn with_node_types(mut v: serde_json::Value) -> serde_json::Value {
    v["contributions"]["nodeTypes"] = serde_json::json!([
        {
            "type_id": "llm-openai",
            "label": "LLM OpenAI",
            "category": "ai",
            "color": "#0d9488",
            "complexity": "complex",
            "output_ports": [{ "name": "default", "label": "Next" }]
        }
    ]);
    v
}

#[test]
fn design_ext_with_gtpack_and_node_types_is_ok() {
    let raw = with_node_types(with_gtpack(skeleton("DesignExtension")));
    let parsed: Result<DescribeJson, _> = serde_json::from_value(raw);
    assert!(parsed.is_ok(), "expected Ok, got: {:?}", parsed.err());
}

#[test]
fn design_ext_with_gtpack_but_no_node_types_is_err() {
    let raw = with_gtpack(skeleton("DesignExtension"));
    let err = serde_json::from_value::<DescribeJson>(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("DesignExtension with `runtime.gtpack`"),
        "expected helpful error, got: {err}"
    );
}

#[test]
fn design_ext_with_node_types_but_no_gtpack_is_ok() {
    let raw = with_node_types(skeleton("DesignExtension"));
    let parsed: Result<DescribeJson, _> = serde_json::from_value(raw);
    assert!(parsed.is_ok(), "expected Ok, got: {:?}", parsed.err());
}

#[test]
fn design_ext_with_empty_node_types_array_plus_gtpack_is_err() {
    let mut raw = with_gtpack(skeleton("DesignExtension"));
    raw["contributions"]["nodeTypes"] = serde_json::json!([]);
    let err = serde_json::from_value::<DescribeJson>(raw).unwrap_err();
    assert!(err.to_string().contains("nodeTypes"), "got: {err}");
}

#[test]
fn bundle_ext_with_gtpack_is_still_err() {
    let raw = with_gtpack(skeleton("BundleExtension"));
    let err = serde_json::from_value::<DescribeJson>(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("only allowed for ProviderExtension")
            || err
                .to_string()
                .contains("only allowed when kind=ProviderExtension"),
        "got: {err}"
    );
}

#[test]
fn deploy_ext_with_gtpack_is_still_err() {
    let raw = with_gtpack(skeleton("DeployExtension"));
    let err = serde_json::from_value::<DescribeJson>(raw).unwrap_err();
    assert!(
        err.to_string()
            .contains("only allowed for ProviderExtension")
            || err
                .to_string()
                .contains("only allowed when kind=ProviderExtension"),
        "got: {err}"
    );
}

#[test]
fn provider_ext_without_gtpack_is_still_err() {
    let raw = skeleton("ProviderExtension");
    let err = serde_json::from_value::<DescribeJson>(raw).unwrap_err();
    assert!(
        err.to_string().contains("requires `runtime.gtpack`"),
        "got: {err}"
    );
}
