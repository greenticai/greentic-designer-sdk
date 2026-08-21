//! The real bundled fallback extensions are all `greentic.ai/v1`. Migrating one
//! must produce a describe that (a) passes the v2 schema and (b) deserializes
//! into the typed `DescribeJson` — otherwise the runtime rejects it at load.
//!
//! This drives `migrate_v0_4_x_value` over the entire captured v1 bundled
//! population (every design/bundle/deploy/provider pack the designer vendors),
//! so a shape the migration doesn't yet convert fails here at `cargo test`
//! instead of at extension load.

use std::path::PathBuf;

use greentic_extension_sdk_contract::{
    DescribeJson, migrate_v0_4_x_value, schema::validate_describe_json,
};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1_bundled")
}

#[test]
fn every_v1_bundled_describe_migrates_validates_and_deserializes() {
    let dir = fixtures_dir();
    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(&dir).expect("read fixtures dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let raw = std::fs::read(&path).expect("read fixture");
        let value: serde_json::Value = serde_json::from_slice(&raw).expect("fixture is JSON");
        assert_eq!(
            value.get("apiVersion").and_then(|v| v.as_str()),
            Some("greentic.ai/v1"),
            "{name} must be a v1 fixture"
        );
        checked += 1;

        let (migrated, _report) = match migrate_v0_4_x_value(&value) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("{name}: migrate failed: {e}"));
                continue;
            }
        };
        if let Err(e) = validate_describe_json(&migrated) {
            failures.push(format!("{name}: v2 schema rejected migrated describe: {e}"));
            continue;
        }
        if let Err(e) = serde_json::from_value::<DescribeJson>(migrated) {
            failures.push(format!("{name}: DescribeJson deserialize failed: {e}"));
        }
    }

    assert!(
        checked >= 19,
        "expected the full bundled population, saw {checked}"
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} v1 bundled describes do not fully migrate:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
