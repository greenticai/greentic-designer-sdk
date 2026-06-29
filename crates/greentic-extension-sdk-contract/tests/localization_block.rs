use greentic_extension_sdk_contract::Locale;
use greentic_extension_sdk_contract::describe::Localization;
use std::str::FromStr;

#[test]
fn parses_localization_block() {
    let v = serde_json::json!({
        "default_locale": "en",
        "strings": {
            "node.adaptive_card.label": { "id": "Kartu Adaptif" },
            "node.adaptive_card.summary": { "id": "Buat kartu" }
        }
    });
    let l: Localization = serde_json::from_value(v).unwrap();
    assert_eq!(l.default_locale.as_str(), "en");
    assert_eq!(l.strings.len(), 2);
    let id = Locale::from_str("id").unwrap();
    let entry = &l.strings["node.adaptive_card.label"];
    assert_eq!(entry.get(&id).map(String::as_str), Some("Kartu Adaptif"));
}
