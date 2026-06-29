use greentic_extension_sdk_contract::Deprecated;

#[test]
fn minimal_form_parses() {
    let v = serde_json::json!({ "since": "1.4.0" });
    let d: Deprecated = serde_json::from_value(v).unwrap();
    assert_eq!(d.since.to_string(), "1.4.0");
    assert!(d.replaced_by.is_none());
    assert!(d.removal_in.is_none());
}

#[test]
fn full_form_parses() {
    let v = serde_json::json!({
        "since": "1.4.0",
        "replaced_by": "greentic.new-thing",
        "removal_in": "2.0.0"
    });
    let d: Deprecated = serde_json::from_value(v).unwrap();
    assert_eq!(d.replaced_by.as_deref(), Some("greentic.new-thing"));
    assert_eq!(d.removal_in.unwrap().to_string(), "2.0.0");
}

#[test]
fn rejects_invalid_since_version() {
    let v = serde_json::json!({ "since": "nope" });
    assert!(serde_json::from_value::<Deprecated>(v).is_err());
}

#[test]
fn roundtrips() {
    let v = serde_json::json!({
        "since": "1.4.0",
        "replaced_by": "x",
        "removal_in": "2.0.0"
    });
    let d: Deprecated = serde_json::from_value(v.clone()).unwrap();
    let back = serde_json::to_value(&d).unwrap();
    assert_eq!(back, v);
}
