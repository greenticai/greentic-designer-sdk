use greentic_extension_sdk_contract::{DescribeJson, ExtensionKind};
use greentic_extension_sdk_registry::lifecycle::{InstallOptions, Installer, TrustPolicy};
use greentic_extension_sdk_registry::local::LocalFilesystemRegistry;
use greentic_extension_sdk_registry::storage::Storage;
use greentic_extension_sdk_registry::types::ExtensionArtifact;
use greentic_extension_sdk_testing::{ExtensionFixtureBuilder, pack_directory};
use tempfile::TempDir;

/// Build an in-memory `.gtxpack` artifact for a Design extension.
fn design_artifact(name: &str, version: &str) -> ExtensionArtifact {
    let fixture = ExtensionFixtureBuilder::new(ExtensionKind::Design, name, version)
        .offer("greentic:perm/x", "1.0.0")
        .with_wasm(b"wasm".to_vec())
        .build()
        .unwrap();
    let tmp = TempDir::new().unwrap();
    let pack = tmp.path().join("p.gtxpack");
    pack_directory(fixture.root(), &pack).unwrap();
    let bytes = std::fs::read(&pack).unwrap();
    let describe: DescribeJson =
        serde_json::from_slice(&std::fs::read(&fixture.describe_path).unwrap()).unwrap();
    ExtensionArtifact {
        name: name.into(),
        version: version.into(),
        describe,
        bytes,
        signature: None,
    }
}

#[test]
fn install_artifact_aborts_when_permission_confirm_declines() {
    let tmp_home = TempDir::new().unwrap();
    let storage = Storage::new(tmp_home.path());
    let reg = LocalFilesystemRegistry::new("local", tmp_home.path());
    let installer = Installer::new(storage, &reg);

    let artifact = design_artifact("greentic.needs-consent", "0.1.0");
    let opts = InstallOptions {
        trust_policy: TrustPolicy::Loose,
        accept_permissions: false,
        force: false,
    };

    // Simulate the user declining at the permission prompt.
    let result = installer.install_artifact_with_confirm(&artifact, opts, |_, _| false);

    assert!(
        matches!(
            result,
            Err(greentic_extension_sdk_registry::error::RegistryError::PermissionDenied { .. })
        ),
        "expected PermissionDenied, got {result:?}"
    );
    // Nothing must be written when consent is declined.
    let dir = tmp_home
        .path()
        .join("extensions/design/greentic.needs-consent-0.1.0");
    assert!(!dir.exists(), "no files should be installed after decline");
}

#[test]
fn install_artifact_proceeds_when_permission_confirm_accepts() {
    let tmp_home = TempDir::new().unwrap();
    let storage = Storage::new(tmp_home.path());
    let reg = LocalFilesystemRegistry::new("local", tmp_home.path());
    let installer = Installer::new(storage, &reg);

    let artifact = design_artifact("greentic.consented", "0.1.0");
    let opts = InstallOptions {
        trust_policy: TrustPolicy::Loose,
        accept_permissions: false,
        force: false,
    };

    installer
        .install_artifact_with_confirm(&artifact, opts, |_, _| true)
        .unwrap();

    let dir = tmp_home
        .path()
        .join("extensions/design/greentic.consented-0.1.0");
    assert!(dir.join("describe.json").exists());
}

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

#[test]
fn begin_install_uses_distinct_staging_dirs() {
    let tmp_home = TempDir::new().unwrap();
    let storage = Storage::new(tmp_home.path());

    // Two installs of the same extension (e.g. concurrent processes) must get
    // distinct staging dirs so neither deletes the other's in-flight staging.
    let (staging_a, final_a) = storage
        .begin_install(ExtensionKind::Design, "greentic.ext", "1.0.0")
        .unwrap();
    let (staging_b, final_b) = storage
        .begin_install(ExtensionKind::Design, "greentic.ext", "1.0.0")
        .unwrap();

    assert_ne!(
        staging_a, staging_b,
        "concurrent installs must not share a staging dir"
    );
    assert_eq!(final_a, final_b, "final install dir is the same extension");
    assert!(
        staging_a.exists() && staging_b.exists(),
        "neither staging dir may be deleted by the other"
    );
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
