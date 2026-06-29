use greentic_extension_sdk_contract::{Locale, LocalizedString};
use std::collections::BTreeMap;
use std::str::FromStr;

#[test]
fn locale_parses_bcp47_basic() {
    assert_eq!(Locale::from_str("en").unwrap().as_str(), "en");
    assert_eq!(Locale::from_str("en-US").unwrap().as_str(), "en-US");
    assert_eq!(
        Locale::from_str("zh-Hant-TW").unwrap().as_str(),
        "zh-Hant-TW"
    );
}

#[test]
fn locale_rejects_empty_and_whitespace() {
    assert!(Locale::from_str("").is_err());
    assert!(Locale::from_str("en US").is_err());
}

#[test]
fn localized_string_plain_serialises_as_string() {
    let ls = LocalizedString::plain("Hello");
    let v = serde_json::to_value(&ls).unwrap();
    assert_eq!(v, serde_json::json!("Hello"));
}

#[test]
fn localized_string_parses_plain_string() {
    let v = serde_json::json!("Hello");
    let ls: LocalizedString = serde_json::from_value(v).unwrap();
    assert_eq!(ls.default(), "Hello");
    assert!(ls.locales().is_empty());
}

#[test]
fn localized_string_parses_object_form() {
    let v = serde_json::json!({
        "default": "Hello",
        "locales": { "id": "Halo", "ja": "こんにちは" }
    });
    let ls: LocalizedString = serde_json::from_value(v).unwrap();
    assert_eq!(ls.default(), "Hello");
    assert_eq!(ls.locales().len(), 2);
    let id = Locale::from_str("id").unwrap();
    assert_eq!(ls.lookup(&id), Some("Halo"));
}

#[test]
fn localized_string_object_roundtrips() {
    let mut locales = BTreeMap::new();
    locales.insert(Locale::from_str("id").unwrap(), "Halo".into());
    let ls = LocalizedString::with_locales("Hello", locales);
    let s = serde_json::to_string(&ls).unwrap();
    let back: LocalizedString = serde_json::from_str(&s).unwrap();
    assert_eq!(back.default(), "Hello");
    assert_eq!(back.lookup(&Locale::from_str("id").unwrap()), Some("Halo"));
}
