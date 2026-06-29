use greentic_extension_sdk_contract::{ComponentId, RuntimeComponent, Sha256};
use std::str::FromStr;

#[test]
fn oci_only_component_parses() {
    let v = serde_json::json!({
        "oci_ref": "oci://ghcr.io/greenticai/components/component-adaptive-card:1.2.0",
        "sha256": "11".repeat(32),
        "world": "greentic:component/adaptive-card@1.2.0"
    });
    let rc: RuntimeComponent = serde_json::from_value(v).unwrap();
    assert_eq!(
        rc.oci_ref.as_deref(),
        Some("oci://ghcr.io/greenticai/components/component-adaptive-card:1.2.0")
    );
    assert!(rc.gtpack.is_none());
    assert_eq!(rc.world, "greentic:component/adaptive-card@1.2.0");
}

#[test]
fn gtpack_only_component_parses() {
    let v = serde_json::json!({
        "gtpack": {
            "file": "runtime/component-llm-openai.gtpack",
            "sha256": "aa".repeat(32),
            "pack_id": "greentic.llm-openai",
            "component_version": "0.6.0"
        },
        "sha256": "22".repeat(32),
        "world": "greentic:component/llm-openai@0.6.0"
    });
    let rc: RuntimeComponent = serde_json::from_value(v).unwrap();
    assert!(rc.oci_ref.is_none());
    let g = rc.gtpack.unwrap();
    assert_eq!(g.pack_id, "greentic.llm-openai");
}

#[test]
fn both_channels_parse() {
    let v = serde_json::json!({
        "oci_ref": "oci://ghcr.io/x/y:1",
        "gtpack": {
            "file": "runtime/x.gtpack",
            "sha256": "bb".repeat(32),
            "pack_id": "x",
            "component_version": "1.0.0"
        },
        "sha256": "33".repeat(32),
        "world": "x:y@1"
    });
    let rc: RuntimeComponent = serde_json::from_value(v).unwrap();
    assert!(rc.oci_ref.is_some());
    assert!(rc.gtpack.is_some());
}

#[test]
fn neither_channel_rejected() {
    let v = serde_json::json!({
        "sha256": "44".repeat(32),
        "world": "x:y@1"
    });
    let r: Result<RuntimeComponent, _> = serde_json::from_value(v);
    assert!(r.is_err(), "must require at least one of oci_ref/gtpack");
}

#[test]
fn map_keyed_by_component_id() {
    let v = serde_json::json!({
        "adaptive-card": {
            "oci_ref": "oci://x:1",
            "sha256": "55".repeat(32),
            "world": "x:1"
        }
    });
    let m: std::collections::BTreeMap<ComponentId, RuntimeComponent> =
        serde_json::from_value(v).unwrap();
    assert!(m.contains_key(&ComponentId::from_str("adaptive-card").unwrap()));
    let sha = m.values().next().unwrap().sha256;
    assert_eq!(sha, Sha256::from_bytes([0x55; 32]));
}
