//! `metadata.id` validation — the single rule shared by `gtdx new`, `gtdx lint`
//! and `gtdx publish`.
//!
//! Every segment must be a WIT kebab-name, because `gtdx new` turns the id into
//! the WIT package name (`greentic.telco-x` -> `greentic:telco-x`) and the
//! Component Model requires each dash-separated word to start with a letter.
//! No namespace is privileged.

use greentic_extension_sdk_contract::extension_id::{
    EXTENSION_ID_PATTERN, is_valid_extension_id, validate_extension_id,
};

#[test]
fn accepts_a_two_segment_id() {
    assert!(validate_extension_id("greentic.telco-x").is_ok());
}

#[test]
fn accepts_a_digit_after_the_first_letter_of_a_word() {
    assert!(validate_extension_id("greentic.aigent3-designer").is_ok());
    assert!(validate_extension_id("greentic.a1-b2").is_ok());
}

/// 1.2.16 allowed this, and it produced `package greentic:3aigent-designer` —
/// which `cargo component build` rejects with `invalid label: dash-separated
/// words must begin with an ASCII lowercase letter`. An id that cannot be built
/// is not a valid id, however happily the schema accepts it.
#[test]
fn rejects_a_word_starting_with_a_digit() {
    let err = validate_extension_id("greentic.3aigent-designer").expect_err("digit-led word");
    let msg = err.to_string();
    assert!(
        msg.contains("\"3aigent\""),
        "should name the bad word: {msg}"
    );
    assert!(
        msg.contains("WIT"),
        "should say where the rule comes from: {msg}"
    );
}

/// The reported failure: the id looked fine (the segment starts with `p`), but
/// its second *word* did not.
#[test]
fn rejects_a_digit_led_word_later_in_a_segment() {
    let err = validate_extension_id("greentic.provider-3aigent").expect_err("digit-led word");
    assert!(err.to_string().contains("\"3aigent\""), "{err}");
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
    assert!(
        msg.contains("\"3aigent\""),
        "should name the bad word: {msg}"
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
        "greentic.aigent3-designer",
        "com.acme.my-ext",
        "greentic.3aigent-designer",
        "greentic.provider-3aigent",
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
/// actually describe the implementation: every segment is a WIT kebab-name.
#[test]
fn published_pattern_matches_the_implementation() {
    assert_eq!(
        EXTENSION_ID_PATTERN,
        "^[a-z][a-z0-9]*(-[a-z][a-z0-9]*)*(\\.[a-z][a-z0-9]*(-[a-z][a-z0-9]*)*)+$"
    );
}
