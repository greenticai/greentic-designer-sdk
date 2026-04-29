use greentic_ext_state::ExtensionState;
use tempfile::TempDir;

#[test]
fn load_returns_default_when_file_missing() {
    let tmp = TempDir::new().unwrap();
    let state = ExtensionState::load(tmp.path()).unwrap();
    // missing file = empty default = everything enabled
    assert!(state.is_enabled("anything", "1.0.0"));
}

#[test]
fn load_parses_existing_state_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("extensions-state.json");
    std::fs::write(
        &path,
        r#"{
            "schema": "1.0",
            "default": { "enabled": { "ext.a@1.0.0": false, "ext.b@2.0.0": true } },
            "tenants": {}
        }"#,
    )
    .unwrap();
    let state = ExtensionState::load(tmp.path()).unwrap();
    assert!(!state.is_enabled("ext.a", "1.0.0"));
    assert!(state.is_enabled("ext.b", "2.0.0"));
    assert!(state.is_enabled("ext.c", "1.0.0")); // default true when absent
}

#[test]
fn set_enabled_then_query() {
    let mut state = ExtensionState::default();
    state.set_enabled("ext.x", "0.1.0", false);
    assert!(!state.is_enabled("ext.x", "0.1.0"));
    state.set_enabled("ext.x", "0.1.0", true);
    assert!(state.is_enabled("ext.x", "0.1.0"));
}

#[test]
fn save_atomic_writes_then_reload_returns_same_data() {
    let tmp = TempDir::new().unwrap();
    let mut state = ExtensionState::default();
    state.set_enabled("ext.x", "0.1.0", false);
    state.save_atomic(tmp.path()).unwrap();

    let reloaded = ExtensionState::load(tmp.path()).unwrap();
    assert!(!reloaded.is_enabled("ext.x", "0.1.0"));
    assert!(reloaded.is_enabled("ext.y", "0.1.0")); // default true
}

#[test]
fn save_atomic_does_not_leave_tmp_or_lock_on_disk() {
    let tmp = TempDir::new().unwrap();
    let state = ExtensionState::default();
    state.save_atomic(tmp.path()).unwrap();

    let names: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "expected one file, got: {names:?}");
    assert_eq!(names[0], "extensions-state.json");
}

#[test]
fn concurrent_writers_do_not_corrupt_file() {
    use std::sync::Arc;
    let tmp = Arc::new(TempDir::new().unwrap());
    let mut handles = vec![];
    for i in 0..10 {
        let tmp = tmp.clone();
        handles.push(std::thread::spawn(move || {
            let mut state = ExtensionState::load(tmp.path()).unwrap();
            state.set_enabled(&format!("ext.{i}"), "0.1.0", i % 2 == 0);
            // Best-effort save; LockContention is acceptable under contention.
            let _ = state.save_atomic(tmp.path());
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    // File must parse cleanly after the dust settles.
    let _final_state = ExtensionState::load(tmp.path()).unwrap();
}
