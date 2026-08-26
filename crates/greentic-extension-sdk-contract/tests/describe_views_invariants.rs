//! Cross-field invariants for `contributions.views[]`, enforced at
//! deserialize time so every consumer gets them — not just authors who
//! remember to run `gtdx lint`.

use greentic_extension_sdk_contract::describe::DescribeJson;

fn describe_with(views: &serde_json::Value, tools: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "apiVersion": "greentic.ai/v2",
        "kind": "DesignExtension",
        "compat": {
            "min_designer_version": ">=1.2.0",
            "min_runner_version": "^1.2.0",
            "contract_version": "1.2.0"
        },
        "metadata": {
            "id": "greentic.example",
            "name": "example",
            "version": "0.1.0",
            "summary": "s",
            "author": { "name": "a" },
            "license": "Apache-2.0"
        },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "components": {
                "main": {
                    "gtpack": {
                        "file": "extension.wasm",
                        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                        "pack_id": "greentic.example",
                        "component_version": "0.1.0"
                    },
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                    "world": "greentic:example/extension@1.0.0"
                }
            },
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
        },
        "contributions": { "views": views, "tools": tools }
    })
}

fn view(id: &str, tools: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "surface": "designer",
        "title_key": "k",
        "title_fallback": "T",
        "entry": "index.html",
        "placement": { "slot": "designer.sidebar" },
        "tools": tools
    })
}

fn tool(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "export": "greentic:extension-design/tools.invoke-tool",
        "runtime_ref": "main"
    })
}

#[test]
fn valid_views_accepted() {
    let d = describe_with(
        &serde_json::json!([view("a", &serde_json::json!(["fetch_usage"]))]),
        &serde_json::json!([tool("fetch_usage")]),
    );
    let parsed: DescribeJson = serde_json::from_value(d).expect("parses");
    assert_eq!(parsed.contributions.views.len(), 1);
}

#[test]
fn duplicate_view_id_rejected() {
    let d = describe_with(
        &serde_json::json!([
            view("dash", &serde_json::json!([])),
            view("dash", &serde_json::json!([]))
        ]),
        &serde_json::json!([]),
    );
    let err = serde_json::from_value::<DescribeJson>(d)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("dash"),
        "the duplicate id should be named: {err}"
    );
}

#[test]
fn view_naming_an_undeclared_tool_rejected() {
    let d = describe_with(
        &serde_json::json!([view("dash", &serde_json::json!(["ghost_tool"]))]),
        &serde_json::json!([tool("fetch_usage")]),
    );
    let err = serde_json::from_value::<DescribeJson>(d)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("ghost_tool"),
        "the dangling tool should be named: {err}"
    );
}
