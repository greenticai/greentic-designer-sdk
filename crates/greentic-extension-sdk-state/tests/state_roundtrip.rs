use greentic_extension_sdk_state::ExtensionState;
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
fn save_atomic_leaves_no_tmp_file() {
    let tmp = TempDir::new().unwrap();
    let state = ExtensionState::default();
    state.save_atomic(tmp.path()).unwrap();

    let names: Vec<String> = std::fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    // The state file must exist and no `.tmp` may linger. The `.lock` file is
    // intentionally persistent — deleting it would race another process holding
    // a lock on the same inode.
    assert!(names.iter().any(|n| n == "extensions-state.json"));
    assert!(
        !names.iter().any(|n| std::path::Path::new(n)
            .extension()
            .is_some_and(|e| e == "tmp")),
        "no .tmp should remain, got: {names:?}"
    );
}

#[test]
fn concurrent_updates_all_survive() {
    use std::sync::Arc;
    let tmp = Arc::new(TempDir::new().unwrap());
    let mut handles = vec![];
    for i in 0..8 {
        let tmp = tmp.clone();
        handles.push(std::thread::spawn(move || {
            let key_id = format!("ext.{i}");
            // Retry through lock contention until this update lands.
            for attempt in 0..1000u64 {
                if ExtensionState::update(tmp.path(), |s| {
                    s.set_enabled(&key_id, "0.1.0", false);
                })
                .is_ok()
                {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5 + attempt % 7));
            }
            panic!("update for {key_id} never succeeded");
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    // Every update must be present — the lock is held across load+save, so no
    // read-modify-write clobbers another writer's change.
    let final_state = ExtensionState::load(tmp.path()).unwrap();
    for i in 0..8 {
        assert!(
            !final_state.is_enabled(&format!("ext.{i}"), "0.1.0"),
            "update for ext.{i} was lost"
        );
    }
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
