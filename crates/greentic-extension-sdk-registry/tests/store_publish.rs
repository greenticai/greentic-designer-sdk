use chrono::Utc;
use greentic_extension_sdk_contract::{
    DescribeJson, ExtensionKind,
    describe::{Author, Capabilities, Engine, Metadata, Permissions, Runtime},
};
use greentic_extension_sdk_registry::publish::PublishRequest;
use greentic_extension_sdk_registry::registry::ExtensionRegistry;
use greentic_extension_sdk_registry::store::GreenticStoreRegistry;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_req() -> PublishRequest {
    PublishRequest {
        ext_id: "com.example.demo".into(),
        ext_name: "demo".into(),
        version: "0.1.0".into(),
        kind: ExtensionKind::Design,
        artifact_bytes: b"fake-pack".to_vec(),
        artifact_sha256: "abc".into(),
        describe: DescribeJson {
            schema_ref: None,
            api_version: "greentic.ai/v1".into(),
            kind: ExtensionKind::Design,
            metadata: Metadata {
                id: "com.example.demo".into(),
                name: "demo".into(),
                version: "0.1.0".into(),
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
        },
        signature: None,
        force: false,
    }
}

#[tokio::test]
async fn publish_without_token_returns_auth_required() {
    let server = MockServer::start().await;
    let reg = GreenticStoreRegistry::new("store", server.uri(), None);
    let err = reg.publish(sample_req()).await.unwrap_err();
    assert!(matches!(
        err,
        greentic_extension_sdk_registry::RegistryError::AuthRequired(_)
    ));
}

#[tokio::test]
async fn publish_success_parses_receipt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/extensions"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "url": "https://store.example.com/api/v1/extensions/com.example.demo/0.1.0",
            "artifactSha256": "abc",
            "publishedAt": Utc::now().to_rfc3339(),
        })))
        .mount(&server)
        .await;
    let reg = GreenticStoreRegistry::new("store", server.uri(), Some("test-token".into()));
    let receipt = reg.publish(sample_req()).await.unwrap();
    assert_eq!(receipt.sha256, "abc");
    assert!(receipt.url.contains("com.example.demo"));
    assert!(!receipt.signed);
}

#[tokio::test]
async fn publish_401_maps_to_auth_required() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/extensions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "code": "unauthorized",
            "error": "unauthorized"
        })))
        .mount(&server)
        .await;
    let reg = GreenticStoreRegistry::new("store", server.uri(), Some("bad".into()));
    let err = reg.publish(sample_req()).await.unwrap_err();
    assert!(matches!(
        err,
        greentic_extension_sdk_registry::RegistryError::AuthRequired(_)
    ));
}

#[tokio::test]
async fn publish_409_maps_to_version_exists() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/extensions"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "existing_sha": "prev-sha"
        })))
        .mount(&server)
        .await;
    let reg = GreenticStoreRegistry::new("store", server.uri(), Some("tok".into()));
    let err = reg.publish(sample_req()).await.unwrap_err();
    match err {
        greentic_extension_sdk_registry::RegistryError::VersionExists { existing_sha } => {
            assert_eq!(existing_sha, "prev-sha");
        }
        other => panic!("expected VersionExists, got {other:?}"),
    }
}
