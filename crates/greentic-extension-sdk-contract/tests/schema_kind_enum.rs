//! `describe-v2.json` hand-maintains the `kind` enum. Nothing generates it
//! from `ExtensionKind`, so a new variant compiles in Rust and then fails
//! every `gtdx validate` and `gtdx publish` — blaming the descriptor, not the
//! schema. This test makes that failure surface here instead, at test time —
//! and, thanks to the exhaustive matches elsewhere over `ExtensionKind`,
//! actually at compile time for a new variant.

use greentic_extension_sdk_contract::ExtensionKind;

/// `wasix:mcp/router` is deliberately absent from describe-v2: those
/// artifacts validate against `describe-mcp-v1.json` instead. Every other
/// kind must be present.
fn kinds_expected_in_v2() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = ExtensionKind::ALL
        .iter()
        .copied()
        .filter(|k| *k != ExtensionKind::WasixMcpRouter)
        .map(ExtensionKind::wire_name)
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn describe_v2_kind_enum_matches_extension_kind() {
    let raw = include_str!("../schemas/describe-v2.json");
    let schema: serde_json::Value = serde_json::from_str(raw).expect("schema is valid JSON");

    let enum_values = schema["properties"]["kind"]["enum"]
        .as_array()
        .expect("describe-v2.json properties.kind.enum is an array");

    let mut actual: Vec<&str> = enum_values
        .iter()
        .map(|v| v.as_str().expect("kind enum entries are strings"))
        .collect();
    actual.sort_unstable();

    assert_eq!(
        actual,
        kinds_expected_in_v2(),
        "describe-v2.json's kind enum has drifted from ExtensionKind. \
         Add the new variant to the schema (and decide whether it needs its \
         own schema file, as wasix:mcp/router does)."
    );
}

#[test]
fn describe_mcp_v1_pins_the_router_kind() {
    let raw = include_str!("../schemas/describe-mcp-v1.json");
    let schema: serde_json::Value = serde_json::from_str(raw).expect("schema is valid JSON");

    assert_eq!(
        schema["properties"]["kind"]["const"].as_str(),
        Some(ExtensionKind::WasixMcpRouter.wire_name()),
        "describe-mcp-v1.json's kind const must match ExtensionKind::WasixMcpRouter"
    );
}
