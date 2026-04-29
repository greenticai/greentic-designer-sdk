use greentic_ext_registry::storage::Storage;
use tempfile::TempDir;

#[test]
fn computes_extension_dir_for_kind() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::new(tmp.path());
    let dir = storage.extension_dir(
        greentic_ext_contract::ExtensionKind::Design,
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
            greentic_ext_contract::ExtensionKind::Design,
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
fn remove_extension_deletes_dir() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::new(tmp.path());
    let (staging, final_dir) = storage
        .begin_install(
            greentic_ext_contract::ExtensionKind::Bundle,
            "greentic.y",
            "2.0.0",
        )
        .unwrap();
    std::fs::write(staging.join("f"), "x").unwrap();
    storage.commit_install(&staging, &final_dir).unwrap();
    assert!(final_dir.exists());

    storage
        .remove_extension(
            greentic_ext_contract::ExtensionKind::Bundle,
            "greentic.y",
            "2.0.0",
        )
        .unwrap();
    assert!(!final_dir.exists());
}
