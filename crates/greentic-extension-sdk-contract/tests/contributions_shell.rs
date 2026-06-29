use greentic_extension_sdk_contract::describe::Contributions;

#[test]
fn empty_object_parses_to_default_contributions() {
    let v = serde_json::json!({});
    let c: Contributions = serde_json::from_value(v).unwrap();
    assert!(c.node_types.is_empty());
    assert!(c.tools.is_empty());
    assert!(c.recipes.is_empty());
    assert!(c.knowledge.is_empty());
    assert!(c.prompts.is_empty());
    assert!(c.schemas.is_empty());
}

#[test]
fn unknown_keys_rejected() {
    let v = serde_json::json!({ "lol_what": [] });
    let r: Result<Contributions, _> = serde_json::from_value(v);
    assert!(r.is_err());
}

#[test]
fn empty_contributions_serialise_to_empty_object() {
    let c = Contributions::default();
    let v = serde_json::to_value(&c).unwrap();
    assert_eq!(v, serde_json::json!({}));
}
