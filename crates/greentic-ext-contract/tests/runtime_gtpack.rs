use greentic_ext_contract::describe::RuntimeGtpack;

#[test]
fn runtime_gtpack_parses_from_json() {
    let json = serde_json::json!({
        "file": "runtime/provider.gtpack",
        "sha256": "a".repeat(64),
        "pack_id": "greentic.provider.telegram",
        "component_version": "0.6.0"
    });
    let rg: RuntimeGtpack = serde_json::from_value(json).unwrap();
    assert_eq!(rg.file, "runtime/provider.gtpack");
    assert_eq!(rg.pack_id, "greentic.provider.telegram");
    assert_eq!(rg.component_version, "0.6.0");
}

#[test]
fn runtime_gtpack_rejects_short_sha256() {
    let json = serde_json::json!({
        "file": "runtime/provider.gtpack",
        "sha256": "abc",
        "pack_id": "greentic.provider.x",
        "component_version": "0.6.0"
    });
    let err = serde_json::from_value::<RuntimeGtpack>(json).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("sha256"));
}

#[test]
fn runtime_gtpack_rejects_non_hex_sha256() {
    let err = serde_json::from_value::<RuntimeGtpack>(serde_json::json!({
        "file": "runtime/provider.gtpack",
        "sha256": "z".repeat(64),
        "pack_id": "greentic.provider.x",
        "component_version": "0.6.0"
    }))
    .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("sha256"));
}
