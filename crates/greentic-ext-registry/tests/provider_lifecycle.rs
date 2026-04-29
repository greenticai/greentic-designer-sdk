mod support;

use greentic_ext_contract::DescribeJson;
use greentic_ext_registry::{
    lifecycle::{InstallOptions, Installer, TrustPolicy},
    local::LocalFilesystemRegistry,
    storage::Storage,
    types::ExtensionArtifact,
};
use tempfile::TempDir;

/// Read `describe.json` from a gtxpack ZIP and return (`DescribeJson`, `raw_zip_bytes`).
fn load_artifact_from_gtxpack(
    path: &std::path::Path,
    name: &str,
    version: &str,
) -> ExtensionArtifact {
    let bytes = std::fs::read(path).unwrap();
    let cursor = std::io::Cursor::new(bytes.clone());
    let mut zip = zip::ZipArchive::new(cursor).unwrap();
    let mut entry = zip.by_name("describe.json").unwrap();
    let mut raw = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut raw).unwrap();
    let describe: DescribeJson = serde_json::from_slice(&raw).unwrap();
    ExtensionArtifact {
        name: name.into(),
        version: version.into(),
        describe,
        bytes,
        signature: None,
    }
}

#[tokio::test]
async fn install_provider_extracts_gtpack_to_providers_gtdx_dir() {
    let tmp = TempDir::new().unwrap();
    let tmp_home = TempDir::new().unwrap();

    let gtpack_bytes = b"fake-gtpack-content".to_vec();
    let sha = support::sha256_hex(&gtpack_bytes);

    let gtxpack_path = support::build_provider_fixture_gtxpack(
        tmp.path(),
        "greentic.provider.fixture",
        "0.1.0",
        &gtpack_bytes,
        &sha,
    );

    let artifact = load_artifact_from_gtxpack(&gtxpack_path, "greentic.provider.fixture", "0.1.0");

    let storage = Storage::new(tmp_home.path());
    let reg = LocalFilesystemRegistry::new("local", tmp.path());
    let installer = Installer::new(storage, &reg);

    installer
        .install_artifact(
            &artifact,
            InstallOptions {
                trust_policy: TrustPolicy::Loose,
                accept_permissions: true,
                force: false,
            },
        )
        .unwrap();

    // The gtpack should be placed in the gtdx provider directory.
    let extracted_pack = tmp_home
        .path()
        .join("runtime/packs/providers/gtdx")
        .join("greentic.provider.fixture-0.1.0.gtpack");
    assert!(
        extracted_pack.exists(),
        "extracted gtpack should exist at {}",
        extracted_pack.display()
    );
    assert_eq!(std::fs::read(&extracted_pack).unwrap(), gtpack_bytes);

    // describe.json should exist in the extension dir.
    let ext_dir = tmp_home
        .path()
        .join("extensions/provider/greentic.provider.fixture-0.1.0");
    assert!(
        ext_dir.join("describe.json").exists(),
        "describe.json should be in extension dir"
    );

    // The gtpack must NOT remain inside the extension dir.
    assert!(
        !ext_dir.join("runtime/provider.gtpack").exists(),
        "gtpack must not be left in the extensions tree"
    );
}

#[tokio::test]
async fn install_provider_rejects_sha256_mismatch() {
    let tmp = TempDir::new().unwrap();
    let tmp_home = TempDir::new().unwrap();

    let real_bytes = b"real-bytes".to_vec();
    let wrong_sha = "a".repeat(64);

    let gtxpack_path = support::build_provider_fixture_gtxpack(
        tmp.path(),
        "greentic.provider.fake",
        "0.1.0",
        &real_bytes,
        &wrong_sha,
    );

    let artifact = load_artifact_from_gtxpack(&gtxpack_path, "greentic.provider.fake", "0.1.0");

    let storage = Storage::new(tmp_home.path());
    let reg = LocalFilesystemRegistry::new("local", tmp.path());
    let installer = Installer::new(storage, &reg);

    let err = installer
        .install_artifact(
            &artifact,
            InstallOptions {
                trust_policy: TrustPolicy::Loose,
                accept_permissions: true,
                force: false,
            },
        )
        .unwrap_err();

    assert!(
        err.to_string().to_lowercase().contains("sha256"),
        "expected sha256 error, got: {err}"
    );
}

#[tokio::test]
async fn install_provider_refuses_conflict_with_manual_pack() {
    let tmp = TempDir::new().unwrap();
    let tmp_home = TempDir::new().unwrap();

    // Pre-populate a manual gtpack with the same pack_id.
    let manual_dir = tmp_home.path().join("runtime/packs/providers/manual");
    std::fs::create_dir_all(&manual_dir).unwrap();
    std::fs::write(
        manual_dir.join("telegram.gtpack"),
        support::encode_gtpack_with_pack_id("greentic.provider.telegram"),
    )
    .unwrap();

    let gtpack_bytes = b"new-bytes".to_vec();
    let sha = support::sha256_hex(&gtpack_bytes);

    let gtxpack_path = support::build_provider_fixture_gtxpack(
        tmp.path(),
        "greentic.provider.telegram",
        "0.1.0",
        &gtpack_bytes,
        &sha,
    );

    let artifact = load_artifact_from_gtxpack(&gtxpack_path, "greentic.provider.telegram", "0.1.0");

    let storage = Storage::new(tmp_home.path());
    let reg = LocalFilesystemRegistry::new("local", tmp.path());
    let installer = Installer::new(storage, &reg);

    let err = installer
        .install_artifact(
            &artifact,
            InstallOptions {
                trust_policy: TrustPolicy::Loose,
                accept_permissions: true,
                force: false,
            },
        )
        .unwrap_err();

    assert!(
        err.to_string().to_lowercase().contains("conflict")
            || err.to_string().to_lowercase().contains("manual"),
        "expected conflict/manual error, got: {err}"
    );
}
