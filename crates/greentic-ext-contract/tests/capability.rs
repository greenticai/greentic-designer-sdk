use greentic_ext_contract::{CapabilityId, CapabilityRef};

#[test]
fn parses_canonical_cap_id() {
    let id: CapabilityId = "greentic:adaptive-cards/validate".parse().unwrap();
    assert_eq!(id.namespace(), "greentic");
    assert_eq!(id.type_path(), "adaptive-cards/validate");
}

#[test]
fn rejects_missing_colon() {
    let err = "greentic-adaptive-cards"
        .parse::<CapabilityId>()
        .unwrap_err();
    assert!(format!("{err}").contains("malformed"), "got {err}");
}

#[test]
fn capability_ref_version_req_is_semver() {
    let cr: CapabilityRef =
        serde_json::from_str(r#"{"id":"greentic:host/logging","version":"^1.0.0"}"#).unwrap();
    assert!(cr.version_req().matches(&"1.5.0".parse().unwrap()));
    assert!(!cr.version_req().matches(&"2.0.0".parse().unwrap()));
}

#[test]
fn wildcard_all_version_matches_everything() {
    let cr: CapabilityRef = serde_json::from_str(r#"{"id":"x:y/z","version":"*"}"#).unwrap();
    assert!(cr.version_req().matches(&"999.999.999".parse().unwrap()));
}
