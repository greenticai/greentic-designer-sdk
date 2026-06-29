use greentic_extension_sdk_registry::storage::Storage;
use tempfile::TempDir;

#[test]
fn computes_extension_dir_for_kind() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::new(tmp.path());
    let dir = storage.extension_dir(
        greentic_extension_sdk_contract::ExtensionKind::Design,
        "greentic.x",
        "1.2.3",
    );
    assert!(dir.ends_with("design/greentic.x-1.2.3"));
}

#[test]
fn stage_and_commit_atomic_move() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::new(tmp.path());
    let (staging, final_dir) = storage
        .begin_install(
            greentic_extension_sdk_contract::ExtensionKind::Design,
            "greentic.x",
            "1.0.0",
        )
        .unwrap();
    std::fs::write(staging.join("file.txt"), "hello").unwrap();
    storage.commit_install(&staging, &final_dir).unwrap();
    assert!(final_dir.join("file.txt").exists());
    assert!(!staging.exists());
}

#[test]
fn commit_replaces_existing_install_and_swaps_contents() {
    // Re-installing over an existing version replaces it cleanly (new content
    // present, stale content gone, no leftover .old backup) — exercising the
    // rename-aside-then-swap path (audit cycle-2 P3).
    let tmp = TempDir::new().unwrap();
    let storage = Storage::new(tmp.path());
    let kind = greentic_extension_sdk_contract::ExtensionKind::Design;

    let (s1, final_dir) = storage.begin_install(kind, "greentic.x", "1.0.0").unwrap();
    std::fs::write(s1.join("old.txt"), "v1").unwrap();
    storage.commit_install(&s1, &final_dir).unwrap();

    let (s2, final_dir2) = storage.begin_install(kind, "greentic.x", "1.0.0").unwrap();
    assert_eq!(final_dir, final_dir2);
    std::fs::write(s2.join("new.txt"), "v2").unwrap();
    storage.commit_install(&s2, &final_dir2).unwrap();

    assert!(final_dir.join("new.txt").exists(), "new content present");
    assert!(!final_dir.join("old.txt").exists(), "stale content gone");
    assert!(
        !final_dir.with_extension("old").exists(),
        "backup must be cleaned up on success"
    );
}

#[test]
fn remove_extension_deletes_dir() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::new(tmp.path());
    let (staging, final_dir) = storage
        .begin_install(
            greentic_extension_sdk_contract::ExtensionKind::Bundle,
            "greentic.y",
            "2.0.0",
        )
        .unwrap();
    std::fs::write(staging.join("f"), "x").unwrap();
    storage.commit_install(&staging, &final_dir).unwrap();
    assert!(final_dir.exists());

    storage
        .remove_extension(
            greentic_extension_sdk_contract::ExtensionKind::Bundle,
            "greentic.y",
            "2.0.0",
        )
        .unwrap();
    assert!(!final_dir.exists());
}
