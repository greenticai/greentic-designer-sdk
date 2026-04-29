use greentic_extension_sdk_registry::oci::OciRegistry;
use greentic_extension_sdk_registry::{ExtensionRegistry, SearchQuery};

#[tokio::test]
async fn oci_registry_search_returns_empty() {
    let reg = OciRegistry::new("test", "ghcr.io", "greenticai/ext", None);
    let results = reg.search(SearchQuery::default()).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn oci_registry_list_versions_returns_empty_stub() {
    let reg = OciRegistry::new("test", "ghcr.io", "greenticai/ext", None);
    let versions = reg.list_versions("greentic.anything").await.unwrap();
    assert!(versions.is_empty());
}

#[tokio::test]
async fn oci_registry_list_by_kind_returns_empty() {
    let reg = OciRegistry::new("test", "ghcr.io", "greenticai/ext", None);
    let results = reg
        .list_by_kind(greentic_extension_sdk_contract::ExtensionKind::Provider)
        .await
        .unwrap();
    assert!(results.is_empty());
}
