//! `metadata.id` validation — the single rule shared by `gtdx new`, `gtdx lint`
//! and `gtdx publish`.
//!
//! The rule mirrors `describe-v2.json`'s `metadata.id` pattern: the first
//! segment must start with a letter (so an id never reads as a number), every
//! later segment may start with a digit, and no namespace is privileged.

use greentic_extension_sdk_contract::extension_id::{
    EXTENSION_ID_PATTERN, is_valid_extension_id, validate_extension_id,
};

#[test]
fn accepts_a_two_segment_id() {
    assert!(validate_extension_id("greentic.telco-x").is_ok());
}

#[test]
fn accepts_a_segment_that_starts_with_a_digit() {
    // The regression this whole rule exists for: `gtdx new 3aigent-designer`
    // used to hand the wizard a default id its own validator rejected.
    assert!(validate_extension_id("greentic.3aigent-designer").is_ok());
}

#[test]
fn accepts_a_namespace_other_than_greentic() {
    assert!(validate_extension_id("com.acme.my-ext").is_ok());
}

#[test]
fn rejects_a_single_segment_id() {
    let err = validate_extension_id("telco-x").expect_err("no dot");
    let msg = err.to_string();
    assert!(msg.contains("telco-x"), "{msg}");
    assert!(msg.contains("at least two"), "{msg}");
    assert!(
        msg.contains("greentic.telco-x"),
        "should suggest a fix: {msg}"
    );
}

#[test]
fn rejects_a_first_segment_starting_with_a_digit() {
    let err = validate_extension_id("3aigent.designer").expect_err("leading digit");
    let msg = err.to_string();
    assert!(msg.contains("first segment"), "{msg}");
    assert!(
        msg.contains("\"3aigent\""),
        "should name the bad segment: {msg}"
    );
}

#[test]
fn rejects_an_empty_segment() {
    let err = validate_extension_id("greentic..telco-x").expect_err("empty segment");
    let msg = err.to_string();
    assert!(msg.contains("empty"), "{msg}");
    // Counted the way an author reads the id, not the way the loop indexes it.
    assert!(msg.contains("segment 2"), "should be 1-based: {msg}");
}

#[test]
fn rejects_a_trailing_dot() {
    assert!(validate_extension_id("greentic.telco-x.").is_err());
}

#[test]
fn rejects_empty() {
    assert!(validate_extension_id("").is_err());
}

#[test]
fn rejects_uppercase_and_says_which_character() {
    let err = validate_extension_id("greentic.Telco").expect_err("uppercase");
    let msg = err.to_string();
    assert!(
        msg.contains('T'),
        "should name the offending character: {msg}"
    );
    assert!(msg.contains("lowercase"), "should hint at the fix: {msg}");
}

#[test]
fn rejects_underscore_and_says_which_character() {
    let err = validate_extension_id("greentic.telco_x").expect_err("underscore");
    let msg = err.to_string();
    assert!(msg.contains('_'), "{msg}");
    assert!(msg.contains('-'), "should suggest a hyphen instead: {msg}");
}

#[test]
fn rejects_whitespace() {
    assert!(validate_extension_id("greentic.telco x").is_err());
}

#[test]
fn is_valid_agrees_with_validate() {
    for id in [
        "greentic.telco-x",
        "greentic.3aigent-designer",
        "com.acme.my-ext",
        "telco-x",
        "3aigent.designer",
        "greentic..x",
        "",
        "greentic.Telco",
    ] {
        assert_eq!(
            is_valid_extension_id(id),
            validate_extension_id(id).is_ok(),
            "disagreement on {id:?}"
        );
    }
}

/// The published pattern is what error messages and docs quote, so it must
/// actually describe the implementation — including the digit-led later
/// segments that the old `(\.[a-z][a-z0-9-]*)+` form forbade.
#[test]
fn published_pattern_matches_the_implementation() {
    assert_eq!(
        EXTENSION_ID_PATTERN,
        "^[a-z][a-z0-9-]*(\\.[a-z0-9][a-z0-9-]*)+$"
    );
}
