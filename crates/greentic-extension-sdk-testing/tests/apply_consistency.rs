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
///
/// The missing element is itself a container ([`{"name":"b"}`]), so B1's
/// container check reports it once at its own path rather than descending
/// into a nonexistent child and reporting `/collections/1/name`.
#[test]
fn a_missing_array_element_is_reported() {
    let planned = r#"{"collections":[{"name":"a"},{"name":"b"}]}"#;
    let observed = r#"{"collections":[{"name":"a"}]}"#;
    let errs =
        assert_apply_consistent(planned, observed).expect_err("a dropped element is a defect");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "/collections/1");
    assert_eq!(errs[0].observed, None);
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
        errs[0].parse_failure,
        "a parse failure must be flagged as one: {:?}",
        errs[0]
    );
}

#[test]
fn unparseable_observed_input_is_also_an_error() {
    let errs = assert_apply_consistent(r"{}", "{not json")
        .expect_err("unparseable observed state must not silently pass");
    assert_eq!(errs.len(), 1);
    assert!(
        errs[0].parse_failure,
        "a parse failure must be flagged as one: {:?}",
        errs[0]
    );
}

/// B3: `path: ""` is also the RFC 6901 root pointer, so a genuine root-level
/// mismatch on a scalar document must NOT be mistaken for a parse failure —
/// `parse_failure` is what disambiguates the two, not an empty `path`.
#[test]
fn a_scalar_root_mismatch_is_not_flagged_as_a_parse_failure() {
    let errs =
        assert_apply_consistent("5", "6").expect_err("a changed scalar root is inconsistent");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "");
    assert!(
        !errs[0].parse_failure,
        "a root-level value mismatch is not a parse failure: {:?}",
        errs[0]
    );
}

/// B2: RFC 6901 requires `~` -> `~0` and `/` -> `~1`, in that order. A key
/// containing either character is plausible (mount paths, header names,
/// namespaced ids) and must not corrupt the pointer.
#[test]
fn a_key_containing_a_slash_is_escaped_in_the_path() {
    let planned = r#"{"a/b":1}"#;
    let observed = r#"{"a/b":2}"#;
    let errs = assert_apply_consistent(planned, observed).expect_err("leaf differs");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "/a~1b");
}

#[test]
fn a_key_containing_a_tilde_is_escaped_in_the_path() {
    let planned = r#"{"a~b":1}"#;
    let observed = r#"{"a~b":2}"#;
    let errs = assert_apply_consistent(planned, observed).expect_err("leaf differs");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "/a~0b");
}

// --- B1: containers themselves must be compared, not just their leaves. ---

/// An empty planned object was iterated zero times, so it was never
/// compared to anything — a dropped `labels: {}` key passed silently.
#[test]
fn an_empty_planned_object_missing_entirely_from_observed_is_inconsistent() {
    let errs = assert_apply_consistent(r#"{"labels":{}}"#, "{}")
        .expect_err("a dropped empty object must be reported");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "/labels");
    assert_eq!(errs[0].observed, None);
}

/// Same hole, different shape: the observed state has the key but it is not
/// an object at all.
#[test]
fn an_empty_planned_object_replaced_by_a_scalar_is_inconsistent() {
    let errs = assert_apply_consistent(r#"{"labels":{}}"#, r#"{"labels":"nope"}"#)
        .expect_err("an object turned scalar must be reported");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "/labels");
    assert_eq!(errs[0].observed, Some(serde_json::json!("nope")));
}

/// Planned `{}` against observed `{}` is a genuine no-op and must still pass.
#[test]
fn matching_empty_objects_pass() {
    assert!(assert_apply_consistent("{}", "{}").is_ok());
}

/// Planned `[]` against observed `[]` is a genuine no-op and must still pass.
#[test]
fn matching_empty_arrays_pass() {
    assert!(assert_apply_consistent("[]", "[]").is_ok());
}

/// A container-kind mismatch (object planned, array observed) is reported at
/// the container's own path rather than silently skipped.
#[test]
fn planned_object_against_observed_array_is_reported_at_the_container_path() {
    let errs = assert_apply_consistent(r#"{"a":1}"#, "[]")
        .expect_err("an object planned against an array observed must be reported");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "");
}
