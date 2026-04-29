use greentic_ext_registry::credentials::Credentials;
use tempfile::TempDir;

#[test]
fn set_get_remove_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("credentials.toml");

    let mut creds = Credentials::default();
    creds.set("greentic-store", "token-abc");
    creds.save(&path).unwrap();

    let reloaded = Credentials::load(&path).unwrap();
    assert_eq!(reloaded.get("greentic-store"), Some("token-abc"));

    let mut reloaded = reloaded;
    let removed = reloaded.remove("greentic-store");
    assert_eq!(removed.as_deref(), Some("token-abc"));
    assert!(reloaded.get("greentic-store").is_none());
}

#[cfg(unix)]
#[test]
fn save_sets_0600_permission() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("credentials.toml");
    let mut creds = Credentials::default();
    creds.set("x", "y");
    creds.save(&path).unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
}
