//! WIT kebab-name checking — the rule every `metadata.id` segment must satisfy,
//! because `id_to_wit_package` turns the id into the WIT package name.

use greentic_extension_sdk_contract::wit_name::{
    WIT_KEBAB_NAME_PATTERN, check_wit_kebab, is_valid_wit_kebab_name,
};

#[test]
fn accepts_a_single_word() {
    assert!(check_wit_kebab("telco").is_ok());
}

#[test]
fn accepts_several_words() {
    assert!(check_wit_kebab("telco-x-tools").is_ok());
}

#[test]
fn accepts_digits_after_the_first_letter_of_a_word() {
    assert!(check_wit_kebab("provider-aigent3").is_ok());
    assert!(check_wit_kebab("a1-b2").is_ok());
}

/// `cargo component build` on a project whose id produced
/// `greentic:provider-3aigent` fails with `invalid label: dash-separated words
/// must begin with an ASCII lowercase letter`.
#[test]
fn rejects_a_word_starting_with_a_digit() {
    let v = check_wit_kebab("provider-3aigent").expect_err("digit-led word");
    let msg = v.to_string();
    assert!(
        msg.contains("\"3aigent\""),
        "should name the bad word: {msg}"
    );
    assert!(msg.contains("lowercase letter"), "{msg}");
}

#[test]
fn rejects_a_first_word_starting_with_a_digit() {
    assert!(check_wit_kebab("3aigent-designer").is_err());
}

#[test]
fn rejects_underscore_and_suggests_a_hyphen() {
    let v = check_wit_kebab("telco_x").expect_err("underscore");
    let msg = v.to_string();
    assert!(msg.contains('_'), "{msg}");
    assert!(msg.contains('-'), "should suggest a hyphen: {msg}");
}

#[test]
fn rejects_uppercase() {
    let v = check_wit_kebab("TelcoX").expect_err("uppercase");
    assert!(v.to_string().contains("lowercase"), "{v}");
}

#[test]
fn rejects_a_dot() {
    // A whole reverse-DNS id is not a single kebab name; callers split on '.'
    // and check each segment.
    assert!(check_wit_kebab("greentic.telco").is_err());
}

#[test]
fn rejects_leading_trailing_and_doubled_hyphens() {
    for bad in ["-telco", "telco-", "telco--x"] {
        assert!(
            check_wit_kebab(bad).is_err(),
            "expected {bad:?} to be rejected"
        );
    }
}

#[test]
fn rejects_empty() {
    assert!(check_wit_kebab("").is_err());
}

#[test]
fn rejects_whitespace() {
    assert!(check_wit_kebab("telco x").is_err());
}

#[test]
fn is_valid_agrees_with_check() {
    for name in [
        "telco",
        "telco-x-tools",
        "provider-aigent3",
        "provider-3aigent",
        "3aigent-designer",
        "telco_x",
        "TelcoX",
        "greentic.telco",
        "-telco",
        "",
    ] {
        assert_eq!(
            is_valid_wit_kebab_name(name),
            check_wit_kebab(name).is_ok(),
            "disagreement on {name:?}"
        );
    }
}

#[test]
fn published_pattern_matches_the_implementation() {
    assert_eq!(WIT_KEBAB_NAME_PATTERN, "^[a-z][a-z0-9]*(-[a-z][a-z0-9]*)*$");
}
