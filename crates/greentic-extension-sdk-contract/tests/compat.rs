use greentic_extension_sdk_contract::Compat;

#[test]
fn parses_valid_compat() {
    let v = serde_json::json!({
        "min_designer_version": ">=1.2.0",
        "min_runner_version": "^0.12.0",
        "contract_version": "1.2.0"
    });
    let c: Compat = serde_json::from_value(v).unwrap();
    assert!(
        c.min_designer_version
            .matches(&semver::Version::parse("1.3.0").unwrap())
    );
    assert!(
        c.min_runner_version
            .matches(&semver::Version::parse("0.12.5").unwrap())
    );
    assert_eq!(c.contract_version.to_string(), "1.2.0");
}

#[test]
fn rejects_invalid_version_req() {
    let v = serde_json::json!({
        "min_designer_version": "not-a-version",
        "min_runner_version": "^0.12.0",
        "contract_version": "1.2.0"
    });
    let r: Result<Compat, _> = serde_json::from_value(v);
    assert!(r.is_err());
}

#[test]
fn rejects_invalid_contract_version() {
    let v = serde_json::json!({
        "min_designer_version": "^1.2.0",
        "min_runner_version": "^0.12.0",
        "contract_version": "not-semver"
    });
    let r: Result<Compat, _> = serde_json::from_value(v);
    assert!(r.is_err());
}

#[test]
fn serializes_back_to_strings() {
    let v = serde_json::json!({
        "min_designer_version": "^1.2.0",
        "min_runner_version": "^0.12.0",
        "contract_version": "1.2.0"
    });
    let c: Compat = serde_json::from_value(v.clone()).unwrap();
    let back = serde_json::to_value(&c).unwrap();
    assert_eq!(back["contract_version"], "1.2.0");
    let _: semver::VersionReq = back["min_designer_version"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let _: semver::VersionReq = back["min_runner_version"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
}
