//! `runtime.permissions.ui` — what a contributed view is allowed to reach.
//!
//! Kept separate from `permissions.network` on purpose. `network` authorises
//! `http.fetch` from inside the WASM guest, where the caller is the
//! extension's own logic. `ui.fetchHosts` authorises requests a human clicking
//! in a browser can trigger, and the response lands in browser-executed code.
//! Same SSRF rules, different blast radius — a reviewer must see two lines.

use greentic_extension_sdk_contract::describe::{ApiGrant, Permissions};

#[test]
fn ui_permissions_parse() {
    let p: Permissions = serde_json::from_value(serde_json::json!({
        "network": [],
        "secrets": [],
        "callExtensionKinds": [],
        "ui": {
            "fetchHosts": ["https://api.example.com/*"],
            "platformApi": [{"method": "GET", "path_pattern": "/api/flows"}]
        }
    }))
    .expect("parses");

    let ui = p.ui.expect("ui block present");
    assert_eq!(ui.fetch_hosts, vec!["https://api.example.com/*"]);
    assert_eq!(
        ui.platform_api,
        vec![ApiGrant {
            method: "GET".to_string(),
            path_pattern: "/api/flows".to_string()
        }]
    );
}

/// Additive: every describe written before `ui` existed must parse, and must
/// not gain the key on the way back out.
#[test]
fn permissions_without_ui_round_trip_unchanged() {
    let original = serde_json::json!({ "network": [], "secrets": [], "callExtensionKinds": [] });
    let p: Permissions = serde_json::from_value(original.clone()).expect("parses");
    assert!(p.ui.is_none());
    let back = serde_json::to_value(&p).expect("serializes");
    assert_eq!(back, original);
}

#[test]
fn unknown_ui_field_is_rejected() {
    let typo = serde_json::json!({
        "network": [], "secrets": [], "callExtensionKinds": [],
        "ui": { "fetchHost": ["https://api.example.com/*"] }
    });
    let err = serde_json::from_value::<Permissions>(typo).unwrap_err();
    assert!(
        err.to_string().contains("fetchHost"),
        "the rejected field should be named: {err}"
    );
}
