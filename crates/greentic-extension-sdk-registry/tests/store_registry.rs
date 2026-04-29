use greentic_extension_sdk_registry::store::GreenticStoreRegistry;
use greentic_extension_sdk_registry::{ExtensionRegistry, SearchQuery};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn store_registry_search_returns_parsed_results() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/extensions"))
        .and(query_param("kind", "design"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "name": "greentic.ac", "latestVersion": "1.6.0", "kind": "DesignExtension",
              "summary": "Adaptive Cards", "downloads": 42 }
        ])))
        .mount(&server)
        .await;

    let reg = GreenticStoreRegistry::new("default", server.uri(), None);
    let q = SearchQuery {
        kind: Some(greentic_extension_sdk_contract::ExtensionKind::Design),
        ..Default::default()
    };
    let results = reg.search(q).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "greentic.ac");
}

#[tokio::test]
async fn store_registry_fetch_downloads_artifact() {
    let server = MockServer::start().await;
    let describe_json = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "DesignExtension",
        "metadata": {
            "id": "greentic.ac", "name": "AC", "version": "1.6.0",
            "summary": "x", "author": { "name": "G" }, "license": "MIT"
        },
        "engine": { "greenticDesigner": "*", "extRuntime": "*" },
        "capabilities": { "offered": [{"id":"greentic:ac/y","version":"1.0.0"}] },
        "runtime": { "component": "extension.wasm", "permissions": {} },
        "contributions": {}
    });

    Mock::given(method("GET"))
        .and(path("/api/v1/extensions/greentic.ac/1.6.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "describe": describe_json,
            "artifactSha256": "deadbeef",
            "publishedAt": "2026-04-17T00:00:00Z",
            "yanked": false
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/extensions/greentic.ac/1.6.0/artifact"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"fake-gtxpack-bytes".to_vec()))
        .mount(&server)
        .await;

    let reg = GreenticStoreRegistry::new("default", server.uri(), None);
    let art = reg.fetch("greentic.ac", "1.6.0").await.unwrap();
    assert_eq!(art.name, "greentic.ac");
    assert_eq!(art.bytes, b"fake-gtxpack-bytes");
}

#[tokio::test]
async fn store_registry_list_versions_returns_empty_for_404() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/extensions/nope"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let reg = GreenticStoreRegistry::new("default", server.uri(), None);
    let versions = reg.list_versions("nope").await.unwrap();
    assert!(versions.is_empty());
}

#[tokio::test]
async fn store_list_by_kind_filters_search_results() {
    let server = MockServer::start().await;
    // Mock search without kind param (list_by_kind calls search(SearchQuery::default()))
    // Returns both Design and Bundle extensions
    Mock::given(method("GET"))
        .and(path("/api/v1/extensions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "name": "greentic.design-demo", "latestVersion": "0.1.0", "kind": "DesignExtension",
              "summary": "Design Extension Demo", "downloads": 10 },
            { "name": "greentic.bundle-demo", "latestVersion": "0.1.0", "kind": "BundleExtension",
              "summary": "Bundle Extension Demo", "downloads": 5 }
        ])))
        .mount(&server)
        .await;

    let reg = GreenticStoreRegistry::new("default", server.uri(), None);

    // list_by_kind(Design) → only design
    let designs = reg
        .list_by_kind(greentic_extension_sdk_contract::ExtensionKind::Design)
        .await
        .unwrap();
    assert_eq!(designs.len(), 1);
    assert_eq!(designs[0].name, "greentic.design-demo");
    assert_eq!(
        designs[0].kind,
        greentic_extension_sdk_contract::ExtensionKind::Design
    );

    // list_by_kind(Bundle) → only bundle
    let bundles = reg
        .list_by_kind(greentic_extension_sdk_contract::ExtensionKind::Bundle)
        .await
        .unwrap();
    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].name, "greentic.bundle-demo");
    assert_eq!(
        bundles[0].kind,
        greentic_extension_sdk_contract::ExtensionKind::Bundle
    );
}
