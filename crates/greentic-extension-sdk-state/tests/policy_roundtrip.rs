use greentic_extension_sdk_state::{ExtensionPolicy, ExtensionState, UpdateMode};
use tempfile::TempDir;

#[test]
fn policy_defaults_to_star_when_absent() {
    let state = ExtensionState::default();
    assert_eq!(state.constraint_for("greentic.foo"), "*");
    assert!(state.policy("greentic.foo").is_none());
}

#[test]
fn set_policy_then_query_and_persist() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    ExtensionState::update(home, |s| {
        s.set_policy(
            "greentic.foo",
            ExtensionPolicy {
                constraint: Some("^2.0".to_string()),
                mode: UpdateMode::Manual,
                last_failed: None,
            },
        );
    })
    .unwrap();

    let reloaded = ExtensionState::load(home).unwrap();
    assert_eq!(reloaded.constraint_for("greentic.foo"), "^2.0");
    assert_eq!(
        reloaded.policy("greentic.foo").unwrap().mode,
        UpdateMode::Manual
    );
}

#[test]
fn record_failed_sets_marker() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    ExtensionState::update(home, |s| {
        s.record_failed("greentic.foo", "2.1.0", "component failed to load");
    })
    .unwrap();
    let reloaded = ExtensionState::load(home).unwrap();
    let lf = reloaded
        .policy("greentic.foo")
        .unwrap()
        .last_failed
        .clone()
        .unwrap();
    assert_eq!(lf.version, "2.1.0");
    assert_eq!(lf.reason, "component failed to load");
}

#[test]
fn reads_legacy_schema_1_0_without_policies() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("extensions-state.json");
    std::fs::write(
        &path,
        r#"{"schema":"1.0","default":{"enabled":{"greentic.foo@1.0.0":true}}}"#,
    )
    .unwrap();
    let state = ExtensionState::load(tmp.path()).unwrap();
    // Legacy file with no `policies` key loads cleanly; policy defaults apply.
    assert_eq!(state.constraint_for("greentic.foo"), "*");
    assert!(state.is_enabled("greentic.foo", "1.0.0"));
}
