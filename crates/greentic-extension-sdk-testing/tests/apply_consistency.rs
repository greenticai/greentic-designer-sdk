//! Spec D11: a plan the apply does not honour is a dry run, not a contract.
//! The platform enforces this; shipping it as a function lets an addon author
//! run the same rule before deploying.

use greentic_extension_sdk_testing::assert_apply_consistent;

#[test]
fn an_apply_that_reached_the_planned_state_is_consistent() {
    let planned = r#"{"collections":[{"name":"docs","size":768}]}"#;
    let observed = r#"{"collections":[{"name":"docs","size":768}]}"#;
    assert!(assert_apply_consistent(planned, observed).is_ok());
}

/// The addon may report more than it planned — a server-assigned id, a
/// timestamp. Extra keys are not a violation.
#[test]
fn extra_keys_in_the_observed_state_are_allowed() {
    let planned = r#"{"collections":[{"name":"docs"}]}"#;
    let observed = r#"{"collections":[{"name":"docs","uuid":"abc","created_at":"now"}]}"#;
    assert!(assert_apply_consistent(planned, observed).is_ok());
}

#[test]
fn a_changed_leaf_is_reported_with_its_path() {
    let planned = r#"{"collections":[{"name":"docs","size":768}]}"#;
    let observed = r#"{"collections":[{"name":"docs","size":1536}]}"#;
    let errs = assert_apply_consistent(planned, observed)
        .expect_err("a changed leaf must be inconsistent");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "/collections/0/size");
    assert_eq!(errs[0].planned, serde_json::json!(768));
    assert_eq!(errs[0].observed, Some(serde_json::json!(1536)));
}

#[test]
fn a_missing_key_is_reported_with_observed_none() {
    let planned = r#"{"collections":[{"name":"docs","size":768}]}"#;
    let observed = r#"{"collections":[{"name":"docs"}]}"#;
    let errs = assert_apply_consistent(planned, observed).expect_err("a dropped key is a defect");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "/collections/0/size");
    assert_eq!(errs[0].observed, None);
}

/// A shorter array means the apply did not create everything it planned.
#[test]
fn a_missing_array_element_is_reported() {
    let planned = r#"{"collections":[{"name":"a"},{"name":"b"}]}"#;
    let observed = r#"{"collections":[{"name":"a"}]}"#;
    let errs =
        assert_apply_consistent(planned, observed).expect_err("a dropped element is a defect");
    assert_eq!(errs[0].path, "/collections/1/name");
}

/// Every violation is reported, not just the first: an author fixing one at a
/// time learns the shape of the problem slowly and expensively.
#[test]
fn every_violation_is_reported_not_just_the_first() {
    let planned = r#"{"a":1,"b":2,"c":3}"#;
    let observed = r#"{"a":9,"b":8,"c":3}"#;
    let errs = assert_apply_consistent(planned, observed).expect_err("two leaves differ");
    assert_eq!(errs.len(), 2);
    let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths.contains(&"/a") && paths.contains(&"/b"),
        "got: {paths:?}"
    );
}

#[test]
fn unparseable_input_is_an_error_not_a_pass() {
    let errs = assert_apply_consistent("{not json", r"{}")
        .expect_err("unparseable planned state must not silently pass");
    assert_eq!(errs.len(), 1);
    assert!(
        errs[0].path.is_empty(),
        "a parse failure has no path: {:?}",
        errs[0]
    );
}
