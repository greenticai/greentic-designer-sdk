use greentic_extension_sdk_contract::migration::MigrationReport;

#[test]
fn empty_report_has_no_warnings() {
    let r = MigrationReport::default();
    assert!(r.warnings.is_empty());
    assert!(r.dropped_keys.is_empty());
}

#[test]
fn report_pushes_warnings() {
    let mut r = MigrationReport::default();
    r.warn("oh no");
    r.dropped("targets");
    assert_eq!(r.warnings.len(), 1);
    assert_eq!(r.dropped_keys, vec!["targets".to_string()]);
}

/// v1 → v2 must not silently drop declared top-level properties.
///
/// `execution` and `localization` are declared in both schemas but were not in
/// the copy list and were not recorded in `dropped_keys`, so a v1
/// `BundleExtension` migrated into a document that passed the v2 schema and
/// deserialized cleanly having lost its dispatch configuration — with the
/// migration reporting zero drops.
#[test]
fn migration_preserves_execution_and_localization() {
    let v1 = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "BundleExtension",
        "metadata": {
            "id": "greentic.bundler",
            "name": "bundler",
            "version": "1.0.0",
            "summary": "x",
            "author": { "name": "a" },
            "license": "MIT"
        },
        "engine": { "greenticDesigner": "^0.1", "extRuntime": "^0.1" },
        "capabilities": { "offered": [], "required": [] },
        "runtime": { "memoryLimitMB": 64, "permissions": {} },
        "execution": { "dispatch": "wasm", "entrypoint": "run" },
        "localization": { "default_locale": "en" }
    });

    let (out, report) =
        greentic_extension_sdk_contract::migrate_v0_4_x_value(&v1).expect("migrate");

    assert_eq!(
        out.get("execution"),
        v1.get("execution"),
        "execution was dropped; report.dropped_keys = {:?}",
        report.dropped_keys
    );
    assert_eq!(
        out.get("localization"),
        v1.get("localization"),
        "localization was dropped"
    );
}

/// Anything the migration does not handle must be reported, not vanish.
#[test]
fn unhandled_top_level_keys_are_reported_as_dropped() {
    let v1 = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "DesignExtension",
        "metadata": {
            "id": "greentic.x", "name": "x", "version": "1.0.0",
            "summary": "x", "author": { "name": "a" }, "license": "MIT"
        },
        "engine": { "greenticDesigner": "^0.1", "extRuntime": "^0.1" },
        "capabilities": { "offered": [], "required": [] },
        "runtime": { "memoryLimitMB": 64, "permissions": {} },
        "somethingUnknown": { "a": 1 }
    });

    let (_out, report) =
        greentic_extension_sdk_contract::migrate_v0_4_x_value(&v1).expect("migrate");
    assert!(
        report.dropped_keys.iter().any(|k| k == "somethingUnknown"),
        "unhandled key vanished silently; dropped_keys = {:?}",
        report.dropped_keys
    );
}
