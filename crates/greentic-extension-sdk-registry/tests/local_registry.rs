use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::local::LocalFilesystemRegistry;
use greentic_extension_sdk_registry::{ExtensionRegistry, SearchQuery};
use greentic_extension_sdk_testing::{ExtensionFixtureBuilder, pack_directory};
use tempfile::TempDir;

#[tokio::test]
async fn local_registry_finds_and_fetches_packed_extension() {
    let tmp = TempDir::new().unwrap();
    let reg_root = tmp.path().to_path_buf();

    let fixture =
        ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.local-demo", "0.1.0")
            .offer("greentic:demo/hi", "1.0.0")
            .with_wasm(b"not-a-real-wasm".to_vec())
            .build()
            .unwrap();
    let pack_path = reg_root.join("greentic.local-demo-0.1.0.gtxpack");
    pack_directory(fixture.root(), &pack_path).unwrap();

    let reg = LocalFilesystemRegistry::new("local", reg_root);

    let results = reg.search(SearchQuery::default()).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "greentic.local-demo");

    let art = reg.fetch("greentic.local-demo", "0.1.0").await.unwrap();
    assert_eq!(art.version, "0.1.0");
    assert!(!art.bytes.is_empty());

    let versions = reg.list_versions("greentic.local-demo").await.unwrap();
    assert_eq!(versions, vec!["0.1.0"]);
}

#[tokio::test]
async fn local_registry_returns_not_found_for_missing() {
    let tmp = TempDir::new().unwrap();
    let reg = LocalFilesystemRegistry::new("local", tmp.path().to_path_buf());
    let err = reg.fetch("greentic.missing", "0.1.0").await.unwrap_err();
    match err {
        greentic_extension_sdk_registry::RegistryError::NotFound { name, version } => {
            assert_eq!(name, "greentic.missing");
            assert_eq!(version, "0.1.0");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn local_list_by_kind_filters_results() {
    let tmp = TempDir::new().unwrap();
    let reg_root = tmp.path().to_path_buf();

    // Fixture 1: Design extension
    let design =
        ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.design-demo", "0.1.0")
            .offer("greentic:demo/hi", "1.0.0")
            .with_wasm(b"stub".to_vec())
            .build()
            .unwrap();
    pack_directory(
        design.root(),
        &reg_root.join("greentic.design-demo-0.1.0.gtxpack"),
    )
    .unwrap();

    // Fixture 2: Bundle extension
    let bundle =
        ExtensionFixtureBuilder::new(ExtensionKind::Bundle, "greentic.bundle-demo", "0.1.0")
            .offer("greentic:demo/pack", "1.0.0")
            .with_wasm(b"stub".to_vec())
            .build()
            .unwrap();
    pack_directory(
        bundle.root(),
        &reg_root.join("greentic.bundle-demo-0.1.0.gtxpack"),
    )
    .unwrap();

    let reg = LocalFilesystemRegistry::new("local", reg_root);

    // list_by_kind(Design) → only design
    let designs = reg.list_by_kind(ExtensionKind::Design).await.unwrap();
    assert_eq!(designs.len(), 1);
    assert_eq!(designs[0].name, "greentic.design-demo");
    assert_eq!(designs[0].kind, ExtensionKind::Design);

    // list_by_kind(Bundle) → only bundle
    let bundles = reg.list_by_kind(ExtensionKind::Bundle).await.unwrap();
    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].name, "greentic.bundle-demo");
    assert_eq!(bundles[0].kind, ExtensionKind::Bundle);

    // list_by_kind(Provider) → empty (no provider fixtures here)
    let providers = reg.list_by_kind(ExtensionKind::Provider).await.unwrap();
    assert!(providers.is_empty());
}
