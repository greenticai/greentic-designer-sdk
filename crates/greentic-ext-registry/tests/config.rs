use greentic_ext_registry::config::{GtdxConfig, RegistryEntry, load, save};
use tempfile::TempDir;

#[test]
fn load_missing_returns_default() {
    let tmp = TempDir::new().unwrap();
    let cfg = load(&tmp.path().join("config.toml")).unwrap();
    assert_eq!(cfg.default.registry, "greentic-store");
    assert_eq!(cfg.default.trust_policy, "normal");
}

#[test]
fn save_and_reload_preserves_registries() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    let mut cfg = GtdxConfig::default();
    cfg.registries.push(RegistryEntry {
        name: "custom".into(),
        url: "https://example.com".into(),
        token_env: Some("MY_TOKEN".into()),
    });
    save(&path, &cfg).unwrap();

    let reloaded = load(&path).unwrap();
    assert_eq!(reloaded.registries.len(), 1);
    assert_eq!(reloaded.registries[0].name, "custom");
    assert_eq!(
        reloaded.registries[0].token_env.as_deref(),
        Some("MY_TOKEN")
    );
}
