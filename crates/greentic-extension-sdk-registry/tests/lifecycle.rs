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

#[tokio::test]
async fn install_refuses_yanked_version_without_force() {
    use greentic_extension_sdk_registry::store::GreenticStoreRegistry;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let describe_json = serde_json::json!({
        "apiVersion": "greentic.ai/v2",
        "kind": "DesignExtension",
        "compat": { "min_designer_version": ">=1.0.0", "min_runner_version": "^0.12.0", "contract_version": "1.2.0" },
        "metadata": { "id": "greentic.yanked", "name": "Y", "version": "1.0.0", "summary": "x", "author": { "name": "G" }, "license": "MIT" },
        "engine": { "greenticDesigner": "*", "extRuntime": "*" },
        "capabilities": { "offered": [{"id":"greentic:y/c","version":"1.0.0"}] },
        "runtime": {
            "memoryLimitMB": 64,
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] },
            "components": {
                "stub": {
                    "oci_ref": "oci://ghcr.io/example/stub:latest",
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "world": "greentic:component/stub@0.1.0"
                }
            }
        },
        "contributions": {}
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/extensions/greentic.yanked/1.0.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "describe": describe_json,
            "artifactSha256": "deadbeef",
            "publishedAt": "2026-04-17T00:00:00Z",
            "yanked": true
        })))
        .mount(&server)
        .await;

    let tmp_home = TempDir::new().unwrap();
    let storage = Storage::new(tmp_home.path());
    let reg = GreenticStoreRegistry::new("default", server.uri(), None);
    let installer = Installer::new(storage, &reg);

    let err = installer
        .install(
            "greentic.yanked",
            "1.0.0",
            InstallOptions {
                trust_policy: TrustPolicy::Loose,
                accept_permissions: true,
                force: false,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            greentic_extension_sdk_registry::error::RegistryError::Yanked { .. }
        ),
        "expected Yanked, got {err}"
    );
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

/// A registry that answers a request for one extension with a different one.
///
/// Modelled on a compromised store: the pack file sits at the path `fetch`
/// resolves for `requested`, but the `describe.json` *inside* it declares a
/// different id and version. Every integrity/authenticity check downstream
/// validates the served describe against itself, so only an explicit
/// request-vs-response comparison catches the substitution.
fn plant_substituted_pack(root: &std::path::Path, requested: (&str, &str), served: (&str, &str)) {
    let fixture = ExtensionFixtureBuilder::new(ExtensionKind::Design, served.0, served.1)
        .offer("greentic:perm/x", "1.0.0")
        .with_wasm(b"wasm".to_vec())
        .build()
        .unwrap();
    std::fs::create_dir_all(root).unwrap();
    let pack = root.join(format!("{}-{}.gtxpack", requested.0, requested.1));
    pack_directory(fixture.root(), &pack).unwrap();
}

#[tokio::test]
async fn install_refuses_an_extension_the_registry_substituted() {
    let tmp_home = TempDir::new().unwrap();
    let reg_root = tmp_home.path().join("registry");
    plant_substituted_pack(
        &reg_root,
        ("greentic.safe-widget", "1.0.0"),
        ("greentic.audit-logger", "9.9.9"),
    );

    let storage = Storage::new(tmp_home.path());
    let reg = LocalFilesystemRegistry::new("local", &reg_root);
    let installer = Installer::new(storage, &reg);

    let result = installer
        .install(
            "greentic.safe-widget",
            "1.0.0",
            InstallOptions {
                trust_policy: TrustPolicy::Loose,
                accept_permissions: true,
                force: false,
            },
        )
        .await;

    match result {
        Err(greentic_extension_sdk_registry::error::RegistryError::IdentityMismatch {
            requested_name,
            requested_version,
            served_name,
            served_version,
        }) => {
            assert_eq!(requested_name, "greentic.safe-widget");
            assert_eq!(requested_version, "1.0.0");
            assert_eq!(served_name, "greentic.audit-logger");
            assert_eq!(served_version, "9.9.9");
        }
        other => panic!("expected IdentityMismatch, got {other:?}"),
    }

    // The substituted extension must not be on disk under either identity.
    for dir in [
        "extensions/design/greentic.audit-logger-9.9.9",
        "extensions/design/greentic.safe-widget-1.0.0",
    ] {
        assert!(
            !tmp_home.path().join(dir).exists(),
            "{dir} was installed despite the identity mismatch"
        );
    }
    // Nothing may be pinned for the substituted publisher either — the check
    // has to run before verify_authenticity for this to hold.
    let trust = tmp_home.path().join("trust/publishers.json");
    assert!(
        !trust.exists(),
        "a publisher key was pinned for a substituted artifact"
    );
}

#[tokio::test]
async fn install_accepts_a_pack_whose_identity_matches_the_request() {
    let tmp_home = TempDir::new().unwrap();
    let reg_root = tmp_home.path().join("registry");
    plant_substituted_pack(
        &reg_root,
        ("greentic.honest", "1.0.0"),
        ("greentic.honest", "1.0.0"),
    );

    let storage = Storage::new(tmp_home.path());
    let reg = LocalFilesystemRegistry::new("local", &reg_root);
    let installer = Installer::new(storage, &reg);

    installer
        .install(
            "greentic.honest",
            "1.0.0",
            InstallOptions {
                trust_policy: TrustPolicy::Loose,
                accept_permissions: true,
                force: false,
            },
        )
        .await
        .expect("a matching identity must still install");

    assert!(
        tmp_home
            .path()
            .join("extensions/design/greentic.honest-1.0.0")
            .exists(),
        "the guard rejected a legitimate install"
    );
}

/// Declining the permission prompt must not leave a publisher key pinned.
///
/// `verify_authenticity` TOFU-pins as a side effect, and it used to run before
/// the consent gate. A user shown "requests Secrets: …" who answered no had
/// still permanently trusted that publisher for that id — and there is no
/// command to undo a pin.
#[test]
fn declining_consent_does_not_pin_the_publisher_key() {
    let tmp_home = TempDir::new().unwrap();
    let storage = Storage::new(tmp_home.path());
    let reg = LocalFilesystemRegistry::new("local", tmp_home.path());
    let installer = Installer::new(storage, &reg);

    let artifact = design_artifact("greentic.declined", "0.1.0");
    let result = installer.install_artifact_with_confirm(
        &artifact,
        InstallOptions {
            trust_policy: TrustPolicy::Loose,
            accept_permissions: false,
            force: false,
        },
        |_, _| false,
    );

    assert!(
        matches!(
            result,
            Err(greentic_extension_sdk_registry::error::RegistryError::PermissionDenied { .. })
        ),
        "expected PermissionDenied, got {result:?}"
    );
    assert!(
        !tmp_home.path().join("trust/publishers.json").exists(),
        "a publisher key was pinned even though consent was declined"
    );
}

/// Integrity is enforced by the public entry point itself, not only by
/// `install`. `ExtensionArtifact` is public, so a host can build one directly.
#[test]
fn install_artifact_rejects_a_tampered_archive() {
    let tmp_home = TempDir::new().unwrap();
    let storage = Storage::new(tmp_home.path());
    let reg = LocalFilesystemRegistry::new("local", tmp_home.path());
    let installer = Installer::new(storage, &reg);

    let mut artifact = design_artifact("greentic.tampered", "0.1.0");
    // Corrupt the archive bytes after the describe was built from them.
    let len = artifact.bytes.len();
    artifact.bytes[len / 2] ^= 0xff;

    let result = installer.install_artifact_with_confirm(
        &artifact,
        InstallOptions {
            trust_policy: TrustPolicy::Loose,
            accept_permissions: true,
            force: false,
        },
        |_, _| true,
    );
    assert!(
        result.is_err(),
        "a tampered archive was installed through the public entry point"
    );
    assert!(
        !tmp_home
            .path()
            .join("extensions/design/greentic.tampered-0.1.0")
            .exists()
    );
}
