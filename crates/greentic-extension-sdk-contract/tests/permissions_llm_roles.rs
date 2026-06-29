use greentic_extension_sdk_contract::describe::Permissions;

#[test]
fn permissions_llm_roles_default_empty_and_roundtrip() {
    // Old describe.json without llmRoles still parses.
    let legacy: Permissions = serde_json::from_str(r"{}").expect("legacy parses");
    assert!(legacy.llm_roles.is_empty());

    // New field round-trips.
    let v = serde_json::json!({ "llmRoles": ["sorla_composer"] });
    let p: Permissions = serde_json::from_value(v).expect("parses");
    assert_eq!(p.llm_roles, vec!["sorla_composer".to_string()]);
    let back = serde_json::to_value(&p).expect("serializes");
    assert_eq!(back["llmRoles"][0], "sorla_composer");
}

#[test]
fn permissions_llm_roles_omitted_when_empty() {
    // When llm_roles is empty, it must not appear in the serialized output
    // (skip_serializing_if = "Vec::is_empty").
    let p = Permissions::default();
    let s = serde_json::to_string(&p).expect("serializes");
    assert!(
        !s.contains("llmRoles") && !s.contains("llm_roles"),
        "llmRoles must be omitted when empty; got: {s}"
    );
}

#[test]
fn permissions_llm_roles_multiple_roles() {
    let v = serde_json::json!({ "llmRoles": ["sorla_composer", "sorla_reviewer"] });
    let p: Permissions = serde_json::from_value(v).expect("parses");
    assert_eq!(p.llm_roles.len(), 2);
    assert_eq!(p.llm_roles[0], "sorla_composer");
    assert_eq!(p.llm_roles[1], "sorla_reviewer");
}

#[test]
fn snake_case_llm_roles_key_is_rejected() {
    // Wire key is camelCase `llmRoles`; deny_unknown_fields makes the snake
    // form a hard error so producers can't drift back.
    let v = serde_json::json!({ "llm_roles": ["sorla_composer"] });
    assert!(serde_json::from_value::<Permissions>(v).is_err());
}
