use greentic_extension_sdk_contract::ComponentId;
use std::str::FromStr;

#[test]
fn parses_simple_id() {
    let id = ComponentId::from_str("adaptive-card").expect("parse ok");
    assert_eq!(id.as_str(), "adaptive-card");
}

#[test]
fn rejects_empty() {
    assert!(ComponentId::from_str("").is_err());
}

#[test]
fn rejects_uppercase() {
    assert!(ComponentId::from_str("Adaptive-Card").is_err());
}

#[test]
fn rejects_whitespace() {
    assert!(ComponentId::from_str("ada card").is_err());
}

#[test]
fn allows_dots_and_dashes_and_underscores() {
    assert!(ComponentId::from_str("greentic.adaptive-card_v2").is_ok());
}

#[test]
fn serde_roundtrips() {
    let id = ComponentId::from_str("foo-bar").unwrap();
    let s = serde_json::to_string(&id).unwrap();
    assert_eq!(s, "\"foo-bar\"");
    let parsed: ComponentId = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed, id);
}

#[test]
fn rejects_invalid_on_deserialize() {
    let r: Result<ComponentId, _> = serde_json::from_str("\"Bad ID\"");
    assert!(r.is_err());
}
