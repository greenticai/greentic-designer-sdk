use chrono::Utc;
use greentic_extension_sdk_contract::{
    DescribeJson, ExtensionKind,
    describe::{Author, Capabilities, Engine, Metadata, Permissions, Runtime},
};
use greentic_extension_sdk_registry::local::LocalFilesystemRegistry;
use greentic_extension_sdk_registry::publish::PublishRequest;

fn sample_describe(version: &str) -> DescribeJson {
    DescribeJson {
        schema_ref: None,
        api_version: "greentic.ai/v1".into(),
        kind: ExtensionKind::Design,
        metadata: Metadata {
            id: "com.example.demo".into(),
            name: "demo".into(),
            version: version.into(),
            summary: "s".into(),
            description: None,
            author: Author {
                name: "a".into(),
                email: None,
                public_key: None,
            },
            license: "MIT".into(),
            homepage: None,
            repository: None,
            keywords: vec![],
            icon: None,
            screenshots: vec![],
        },
        engine: Engine {
            greentic_designer: "^0.1".into(),
            ext_runtime: "^0.1".into(),
        },
        capabilities: Capabilities {
            offered: vec![],
            required: vec![],
        },
        runtime: Runtime {
            component: "extension.wasm".into(),
            memory_limit_mb: 64,
            permissions: Permissions::default(),
            gtpack: None,
        },
        execution: None,
        contributions: serde_json::json!({}),
        signature: None,
    }
}

fn sample_req(version: &str, force: bool) -> PublishRequest {
    PublishRequest {
        ext_id: "com.example.demo".into(),
        ext_name: "demo".into(),
        version: version.into(),
        kind: ExtensionKind::Design,
        artifact_bytes: b"fake-pack-bytes".to_vec(),
        artifact_sha256: "abc".into(),
        describe: sample_describe(version),
        signature: None,
        force,
    }
}

#[test]
fn publish_writes_expected_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let reg = LocalFilesystemRegistry::new("test", tmp.path().to_path_buf());
    let receipt = reg.publish_local(&sample_req("0.1.0", false)).unwrap();
    assert!(receipt.url.starts_with("file://"));
    assert!(receipt.published_at <= Utc::now());
    assert!(!receipt.signed);

    let ver = tmp.path().join("com.example.demo/0.1.0");
    assert!(ver.join("demo-0.1.0.gtxpack").exists());
    assert!(ver.join("manifest.json").exists());
    assert!(ver.join("artifact.sha256").exists());
    assert!(tmp.path().join("index.json").exists());
    assert!(tmp.path().join("com.example.demo/metadata.json").exists());
}

#[test]
fn duplicate_version_without_force_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let reg = LocalFilesystemRegistry::new("test", tmp.path().to_path_buf());
    reg.publish_local(&sample_req("0.1.0", false)).unwrap();
    let err = reg.publish_local(&sample_req("0.1.0", false)).unwrap_err();
    assert!(matches!(
        err,
        greentic_extension_sdk_registry::RegistryError::VersionExists { .. }
    ));
}

#[test]
fn force_overwrites_existing_version() {
    let tmp = tempfile::tempdir().unwrap();
    let reg = LocalFilesystemRegistry::new("test", tmp.path().to_path_buf());
    reg.publish_local(&sample_req("0.1.0", false)).unwrap();
    let mut req = sample_req("0.1.0", true);
    req.artifact_bytes = b"fake-pack-v2".to_vec();
    req.artifact_sha256 = "xyz".into();
    let receipt = reg.publish_local(&req).unwrap();
    assert_eq!(receipt.sha256, "xyz");
    let sha_sidecar = tmp.path().join("com.example.demo/0.1.0/artifact.sha256");
    assert_eq!(std::fs::read_to_string(&sha_sidecar).unwrap().trim(), "xyz");
}

#[test]
fn index_tracks_multiple_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let reg = LocalFilesystemRegistry::new("test", tmp.path().to_path_buf());
    reg.publish_local(&sample_req("0.1.0", false)).unwrap();
    reg.publish_local(&sample_req("0.1.1", false)).unwrap();
    let idx_bytes = std::fs::read(tmp.path().join("index.json")).unwrap();
    let idx: serde_json::Value = serde_json::from_slice(&idx_bytes).unwrap();
    let exts = idx["extensions"].as_array().unwrap();
    assert_eq!(exts.len(), 1);
    let versions = exts[0]["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(exts[0]["latest"], "0.1.1");
}
