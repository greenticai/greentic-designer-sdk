//! `contributions` is `additionalProperties: false`, so a new contribution
//! slot is invisible to the schema until it is declared there. Without this,
//! a describe that deserializes perfectly still fails `gtdx validate`.

use greentic_extension_sdk_contract::schema::validate_describe_json;

fn base() -> serde_json::Value {
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
        "contributions": {}
    })
}

#[test]
fn describe_with_a_view_validates() {
    let mut d = base();
    d["contributions"]["views"] = serde_json::json!([{
        "id": "usage-dashboard",
        "surface": "admin",
        "title_key": "view.usage.label",
        "title_fallback": "Usage",
        "entry": "index.html",
        "placement": { "slot": "admin.sidebar", "path": ["Governance"], "order": 10 },
        "min_visibility": "tenant_admin",
        "tools": []
    }]);
    validate_describe_json(&d).expect("a describe carrying views must validate");
}

#[test]
fn describe_with_ui_permissions_validates() {
    let mut d = base();
    d["runtime"]["permissions"]["ui"] = serde_json::json!({
        "fetchHosts": ["https://api.example.com/*"],
        "platformApi": [{ "method": "GET", "path_pattern": "/api/flows" }]
    });
    validate_describe_json(&d).expect("a describe carrying permissions.ui must validate");
}

#[test]
fn view_missing_required_field_is_rejected() {
    let mut d = base();
    d["contributions"]["views"] = serde_json::json!([{
        "id": "no-entry",
        "surface": "admin",
        "title_key": "k",
        "title_fallback": "T",
        "placement": { "slot": "admin.sidebar" }
    }]);
    assert!(
        validate_describe_json(&d).is_err(),
        "a view without `entry` must not validate"
    );
}

#[test]
fn unknown_surface_is_rejected_by_schema() {
    let mut d = base();
    d["contributions"]["views"] = serde_json::json!([{
        "id": "x",
        "surface": "mobile",
        "title_key": "k",
        "title_fallback": "T",
        "entry": "index.html",
        "placement": { "slot": "admin.sidebar" }
    }]);
    assert!(validate_describe_json(&d).is_err());
}

#[test]
fn unknown_api_grant_method_is_rejected_by_schema() {
    let mut d = base();
    d["runtime"]["permissions"]["ui"] = serde_json::json!({
        "platformApi": [{ "method": "TRACE", "path_pattern": "/api/flows" }]
    });
    assert!(validate_describe_json(&d).is_err());
}
