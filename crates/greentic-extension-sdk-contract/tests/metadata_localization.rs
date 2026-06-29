use greentic_extension_sdk_contract::Locale;
use greentic_extension_sdk_contract::describe::Metadata;
use std::str::FromStr;

#[test]
fn metadata_summary_accepts_plain_string() {
    let v = serde_json::json!({
        "id": "greentic.x",
        "name": "X",
        "version": "0.1.0",
        "summary": "Plain summary",
        "author": { "name": "Greentic" },
        "license": "MIT"
    });
    let m: Metadata = serde_json::from_value(v).unwrap();
    assert_eq!(m.summary.default(), "Plain summary");
    assert!(m.summary.locales().is_empty());
}

#[test]
fn metadata_summary_accepts_localized_object() {
    let v = serde_json::json!({
        "id": "greentic.x",
        "name": "X",
        "version": "0.1.0",
        "summary": { "default": "Plain", "locales": { "id": "Polos" } },
        "description": { "default": "Long", "locales": { "id": "Panjang" } },
        "author": { "name": "Greentic" },
        "license": "MIT"
    });
    let m: Metadata = serde_json::from_value(v).unwrap();
    let id = Locale::from_str("id").unwrap();
    assert_eq!(m.summary.lookup(&id), Some("Polos"));
    assert_eq!(m.description.unwrap().lookup(&id), Some("Panjang"));
}
