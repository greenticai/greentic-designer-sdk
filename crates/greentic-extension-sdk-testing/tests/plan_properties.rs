//! `plan(x, x)` returning anything but `x` unchanged means the addon is not
//! idempotent, which means it never converges. One property, checked
//! mechanically, with no infrastructure.

use greentic_extension_sdk_testing::{PlanResult, assert_plan_idempotent, assert_plan_stable};

/// A well-behaved addon: planning current against itself is a no-op.
///
/// Always returns `Some`: it exists to satisfy the `Fn(&str, &str) ->
/// Option<PlanResult>` closure bound both assertions take, not because this
/// particular addon ever defers.
#[allow(clippy::unnecessary_wraps)]
fn good_plan(_current: &str, desired: &str) -> Option<PlanResult> {
    Some(PlanResult {
        planned_json: desired.to_string(),
        requires_replace: Vec::new(),
    })
}

#[test]
fn an_idempotent_plan_passes() {
    let current = r#"{"collections":[{"name":"docs"}]}"#;
    assert!(assert_plan_idempotent(current, good_plan).is_ok());
}

/// The failure this property exists to catch: an addon that always proposes a
/// change, so every reconcile has work to do and the resource never settles.
#[test]
fn a_plan_that_always_proposes_a_change_fails() {
    let churning = |_c: &str, _d: &str| {
        Some(PlanResult {
            planned_json: r#"{"collections":[{"name":"docs","touched":true}]}"#.to_string(),
            requires_replace: Vec::new(),
        })
    };
    let err = assert_plan_idempotent(r#"{"collections":[{"name":"docs"}]}"#, churning)
        .expect_err("a churning plan must fail");
    assert!(
        err.contains("touched"),
        "the message should show the diff: {err}"
    );
}

/// `requires-replace` on a no-op plan means the addon would destroy and
/// recreate a resource that already matches.
#[test]
fn a_no_op_plan_that_requires_replace_fails() {
    let destructive = |_c: &str, d: &str| {
        Some(PlanResult {
            planned_json: d.to_string(),
            requires_replace: vec!["/collections/0".to_string()],
        })
    };
    let err = assert_plan_idempotent(r#"{"collections":[{"name":"docs"}]}"#, destructive)
        .expect_err("a no-op plan must not require replacement");
    assert!(err.contains("requires-replace"), "got: {err}");
}

/// `deferred` is a legitimate answer, but not to `plan(x, x)`: nothing is
/// missing when current and desired already agree.
#[test]
fn deferring_an_identity_plan_fails() {
    let deferring = |_c: &str, _d: &str| None;
    let err = assert_plan_idempotent(r#"{"a":1}"#, deferring)
        .expect_err("deferring an identity plan must fail");
    assert!(err.contains("deferred"), "got: {err}");
}

#[test]
fn a_stable_plan_passes() {
    assert!(assert_plan_stable(r#"{"a":1}"#, r#"{"a":2}"#, good_plan).is_ok());
}

/// A plan that varies between identical calls cannot be approved: what the
/// user saw is not what the apply will do.
#[test]
fn a_plan_that_varies_between_calls_fails() {
    let counter = std::cell::Cell::new(0);
    let unstable = |_c: &str, _d: &str| {
        let n = counter.get();
        counter.set(n + 1);
        Some(PlanResult {
            planned_json: format!(r#"{{"call":{n}}}"#),
            requires_replace: Vec::new(),
        })
    };
    let err = assert_plan_stable(r#"{"a":1}"#, r#"{"a":2}"#, unstable)
        .expect_err("an unstable plan must fail");
    assert!(err.contains("differed"), "got: {err}");
}
