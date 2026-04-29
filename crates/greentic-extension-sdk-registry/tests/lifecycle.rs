use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::lifecycle::{InstallOptions, Installer, TrustPolicy};
use greentic_extension_sdk_registry::local::LocalFilesystemRegistry;
use greentic_extension_sdk_registry::storage::Storage;
use greentic_extension_sdk_testing::{ExtensionFixtureBuilder, pack_directory};
use tempfile::TempDir;

#[tokio::test]
async fn installs_from_local_registry() {
    let tmp_reg = TempDir::new().unwrap();
    let tmp_home = TempDir::new().unwrap();

    let fixture =
        ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.install-me", "0.1.0")
            .offer("greentic:im/hi", "1.0.0")
            .with_wasm(b"wasm".to_vec())
            .build()
            .unwrap();
    let pack = tmp_reg.path().join("greentic.install-me-0.1.0.gtxpack");
    pack_directory(fixture.root(), &pack).unwrap();

    let reg = LocalFilesystemRegistry::new("local", tmp_reg.path());
    let storage = Storage::new(tmp_home.path());
    let installer = Installer::new(storage, &reg);

    installer
        .install(
            "greentic.install-me",
            "0.1.0",
            InstallOptions {
                trust_policy: TrustPolicy::Loose,
                accept_permissions: true,
                force: false,
            },
        )
        .await
        .unwrap();

    let dir = tmp_home
        .path()
        .join("extensions/design/greentic.install-me-0.1.0");
    assert!(dir.join("describe.json").exists());
    assert!(dir.join("extension.wasm").exists());
}

#[tokio::test]
async fn uninstall_removes_dir() {
    let tmp_home = TempDir::new().unwrap();
    let storage = Storage::new(tmp_home.path());

    let (staging, final_dir) = storage
        .begin_install(ExtensionKind::Bundle, "greentic.bye", "0.1.0")
        .unwrap();
    std::fs::write(staging.join("describe.json"), "{}").unwrap();
    storage.commit_install(&staging, &final_dir).unwrap();
    assert!(final_dir.exists());

    let reg = LocalFilesystemRegistry::new("local", tmp_home.path());
    let installer = Installer::new(storage.clone_shallow(), &reg);
    installer
        .uninstall(ExtensionKind::Bundle, "greentic.bye", "0.1.0")
        .unwrap();
    assert!(!final_dir.exists());
}
