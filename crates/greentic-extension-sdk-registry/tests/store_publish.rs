use chrono::Utc;
use greentic_extension_sdk_contract::{
    Compat, DescribeJson, ExtensionKind, LocalizedString,
    describe::{Author, Capabilities, Contributions, Engine, Metadata, Permissions, Runtime},
};
use greentic_extension_sdk_registry::publish::PublishRequest;
use greentic_extension_sdk_registry::registry::ExtensionRegistry;
use greentic_extension_sdk_registry::store::GreenticStoreRegistry;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn default_compat() -> Compat {
    Compat {
        min_designer_version: ">=1.0.0".parse().unwrap(),
        min_runner_version: "^0.12.0".parse().unwrap(),
        contract_version: "1.2.0".parse().unwrap(),
    }
}

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
            api_version: "greentic.ai/v2".into(),
            kind: ExtensionKind::Design,
            compat: default_compat(),
            metadata: Metadata {
                id: "com.example.demo".into(),
                name: "demo".into(),
                version: "0.1.0".into(),
                summary: LocalizedString::plain("s"),
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
            engine: Some(Engine {
                greentic_designer: "^0.1".into(),
                ext_runtime: "^0.1".into(),
            }),
            capabilities: Capabilities {
                offered: vec![],
                required: vec![],
            },
            runtime: Runtime {
                memory_limit_mb: 64,
                permissions: Permissions::default(),
                components: std::collections::BTreeMap::new(),
            },
            execution: None,
            contributions: Contributions::default(),
            localization: None,
            signature: None,
            manifest_sha256: None,
            required_secrets: vec![],
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

#[tokio::test]
async fn publish_2xx_with_garbage_body_errors_not_fabricated() {
    // A 2xx response whose body isn't the expected JSON receipt must surface an
    // error, not be silently turned into a fabricated success receipt built from
    // client-side fallbacks (audit cycle-1 P1-5).
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/extensions"))
        .respond_with(ResponseTemplate::new(201).set_body_string("<<not json>>"))
        .mount(&server)
        .await;
    let reg = GreenticStoreRegistry::new("store", server.uri(), Some("tok".into()));
    assert!(
        reg.publish(sample_req()).await.is_err(),
        "a 2xx with a non-JSON body must not fabricate a success receipt"
    );
}

/// A `wasix:mcp/router` describe.json must deserialize into `ExtensionKind::WasixMcpRouter`
/// and a `PublishRequest` built from it must carry the correct kind — confirming
/// the SDK-side enum handles the `:` and `/` in the kind string via serde rename.
#[test]
fn wasix_mcp_router_describe_deserializes_and_publish_request_carries_kind() {
    // Minimal describe.json with kind = "wasix:mcp/router" (as emitted by
    // `gtdx new --kind mcp` from Task 1). runtime.components must have at least
    // one entry (invariant enforced by DescribeJson::try_from).
    let describe_json = r#"{
      "apiVersion": "greentic.ai/v2",
      "kind": "wasix:mcp/router",
      "compat": {"min_designer_version": ">=1.0.0", "min_runner_version": "^0.12.0", "contract_version": "1.2.0"},
      "metadata": {"id": "com.example.my-mcp", "name": "my-mcp", "version": "0.1.0", "summary": "MCP router", "author": {"name": "a"}, "license": "MIT"},
      "capabilities": {"offered": [], "required": []},
      "runtime": {
        "components": {
          "router": {
            "oci_ref": "oci://ghcr.io/example/my-mcp:0.1.0",
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "world": "wasix:mcp/router@25.06.18"
          }
        },
        "permissions": {"network": [], "secrets": [], "callExtensionKinds": []}
      },
      "contributions": {}
    }"#;

    let describe: DescribeJson = serde_json::from_str(describe_json)
        .expect("wasix:mcp/router describe.json must deserialize into DescribeJson");

    assert_eq!(
        describe.kind,
        ExtensionKind::WasixMcpRouter,
        "describe.kind must be WasixMcpRouter after deserialization"
    );

    // Round-trip: serialize back and confirm the kind string is preserved.
    let re_serialized = serde_json::to_value(&describe).unwrap();
    assert_eq!(
        re_serialized["kind"],
        serde_json::Value::String("wasix:mcp/router".into()),
        "re-serialized kind must be the literal string 'wasix:mcp/router'"
    );

    // A PublishRequest built from this describe carries the correct kind — this
    // is what `gtdx publish` constructs before sending to the registry backend.
    let req = PublishRequest {
        ext_id: describe.metadata.id.clone(),
        ext_name: describe.metadata.name.clone(),
        version: describe.metadata.version.clone(),
        kind: describe.kind,
        artifact_bytes: b"fake-wasm".to_vec(),
        artifact_sha256: "abc".into(),
        describe: describe.clone(),
        signature: None,
        force: false,
    };
    assert_eq!(
        req.kind,
        ExtensionKind::WasixMcpRouter,
        "PublishRequest.kind must be WasixMcpRouter for a wasix:mcp/router describe"
    );
}
