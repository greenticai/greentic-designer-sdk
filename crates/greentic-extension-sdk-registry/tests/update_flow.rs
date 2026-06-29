use std::collections::HashMap;

use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::lifecycle::{InstallOptions, TrustPolicy};
use greentic_extension_sdk_registry::local::LocalFilesystemRegistry;
use greentic_extension_sdk_registry::storage::Storage;
use greentic_extension_sdk_registry::update::{UpdateStatus, check_updates, upgrade};
use greentic_extension_sdk_testing::{ExtensionFixtureBuilder, pack_directory};
use std::path::Path;

fn publish_pack(reg_dir: &Path, name: &str, version: &str) {
    let fixture = ExtensionFixtureBuilder::new(ExtensionKind::Design, name, version)
        .offer("greentic:perm/x", "1.0.0")
        .with_wasm(b"wasm".to_vec())
        .build()
        .unwrap();
    let pack = reg_dir.join(format!("{name}-{version}.gtxpack"));
    pack_directory(fixture.root(), &pack).unwrap();
}

#[tokio::test]
async fn check_updates_reports_available() {
    let tmp = tempfile::TempDir::new().unwrap();
    let reg_dir = tmp.path().join("reg");
    std::fs::create_dir_all(&reg_dir).unwrap();
    publish_pack(&reg_dir, "greentic.foo", "1.0.0");
    publish_pack(&reg_dir, "greentic.foo", "1.1.0");
    let reg = LocalFilesystemRegistry::new("test", &reg_dir);

    let installed = vec![(
        ExtensionKind::Design,
        "greentic.foo".to_string(),
        "1.0.0".to_string(),
    )];
    let constraints = HashMap::new(); // defaults to "*"
    let updates = check_updates(&reg, &installed, &constraints).await;

    assert_eq!(updates.len(), 1);
    assert_eq!(
        updates[0].status,
        UpdateStatus::UpdateAvailable {
            target: "1.1.0".into(),
            is_major_jump: false
        }
    );
}

#[tokio::test]
async fn upgrade_installs_target_and_removes_old() {
    let tmp = tempfile::TempDir::new().unwrap();
    let home = tmp.path().join("home");
    let reg_dir = tmp.path().join("reg");
    std::fs::create_dir_all(&reg_dir).unwrap();
    publish_pack(&reg_dir, "greentic.foo", "1.0.0");
    publish_pack(&reg_dir, "greentic.foo", "1.1.0");
    let reg = LocalFilesystemRegistry::new("test", &reg_dir);
    let storage = Storage::new(&home);

    // Seed the old version on disk.
    let opts = InstallOptions {
        trust_policy: TrustPolicy::Loose,
        accept_permissions: true,
        force: false,
    };
    upgrade(
        &storage,
        &reg,
        ExtensionKind::Design,
        "greentic.foo",
        "0.0.0",
        "1.0.0",
        opts,
    )
    .await
    .unwrap();
    assert!(
        storage
            .extension_dir(ExtensionKind::Design, "greentic.foo", "1.0.0")
            .exists()
    );

    // Upgrade 1.0.0 -> 1.1.0.
    upgrade(
        &storage,
        &reg,
        ExtensionKind::Design,
        "greentic.foo",
        "1.0.0",
        "1.1.0",
        opts,
    )
    .await
    .unwrap();

    assert!(
        storage
            .extension_dir(ExtensionKind::Design, "greentic.foo", "1.1.0")
            .exists()
    );
    assert!(
        !storage
            .extension_dir(ExtensionKind::Design, "greentic.foo", "1.0.0")
            .exists()
    );
}
