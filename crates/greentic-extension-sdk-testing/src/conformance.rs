//! Conformance checks an addon can run before deploying.
//!
//! The rule in [`assert_apply_consistent`] is the one the platform enforces in
//! production. It lives here so an author runs the same rule rather than a
//! test that resembles it.

use serde_json::Value;

/// One leaf where the applied state disagrees with the plan that was approved.
///
/// `#[non_exhaustive]`: only this crate constructs it, so a future field is
/// additive rather than breaking.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Inconsistency {
    /// JSON Pointer to the offending leaf.
    ///
    /// `""` is the RFC 6901 root pointer, which is a legitimate value here —
    /// a scalar-root document that disagrees at the root reports `path:
    /// ""` — so an empty path does NOT by itself mean the input failed to
    /// parse. Check [`Self::parse_failure`] for that instead.
    pub path: String,
    /// What the plan said. For a parse failure, a human-readable message
    /// describing what went wrong, wrapped as a JSON string.
    pub planned: Value,
    /// What the apply produced. `None` when the key is absent entirely, or
    /// when this entry reports a parse failure.
    pub observed: Option<Value>,
    /// `true` when this entry reports that `planned_json` or `observed_json`
    /// did not parse as JSON at all, rather than a genuine mismatch at
    /// `path`. See the note on [`Self::path`]: the parse-failure sentinel
    /// used to be "`path` is empty", which collided with a real root-level
    /// mismatch. This field disambiguates the two without repurposing
    /// `path`.
    pub parse_failure: bool,
}

/// Assert that an apply honoured the plan it was given.
///
/// Every leaf in `planned_json` must appear at the same JSON Pointer in
/// `observed_json` with an equal value. Extra object keys in the observed
/// state are allowed, and so are extra array elements past the length of the
/// planned array: an addon may report a server-assigned id, a timestamp, or
/// more items than it was asked to plan, none of which it could have known in
/// advance.
///
/// This assertion is **subset-only**. It cannot catch an apply that fails to
/// remove something: if `planned` no longer contains a key `current` had,
/// that absence is not itself checked here, because the key never appears in
/// `planned` to compare against `observed` — `observed` may still contain the
/// stale value and this function reports no inconsistency. The platform
/// catches that by diffing `current` against `observed` at apply time, which
/// needs the pre-apply state and is out of scope for a function that only
/// ever sees two documents.
///
/// JSON number comparison is structural: `Number(1)` and `Number(1.0)`
/// compare unequal, because `serde_json::Value` preserves the integer/float
/// distinction from the source text. An addon whose `observe` round-trips
/// state through a REST API or a JS runtime — either of which may normalize
/// `1` to `1.0` — will see that as a mismatch here.
///
/// Returns every violation rather than the first, because an author fixing
/// them one at a time learns the shape of the problem slowly.
///
/// # Errors
///
/// Returns the list of disagreeing leaves. A single entry with
/// [`Inconsistency::parse_failure`] set means one of the two inputs did not
/// parse as JSON — reported as a failure rather than a pass, since silently
/// accepting unparseable state is how a check stops checking.
pub fn assert_apply_consistent(
    planned_json: &str,
    observed_json: &str,
) -> Result<(), Vec<Inconsistency>> {
    let planned: Value = serde_json::from_str(planned_json).map_err(|e| {
        vec![Inconsistency {
            path: String::new(),
            planned: Value::String(format!("planned state is not valid JSON: {e}")),
            observed: None,
            parse_failure: true,
        }]
    })?;
    let observed: Value = serde_json::from_str(observed_json).map_err(|e| {
        vec![Inconsistency {
            path: String::new(),
            planned: Value::String(format!("observed state is not valid JSON: {e}")),
            observed: None,
            parse_failure: true,
        }]
    })?;

    let mut out = Vec::new();
    walk(&planned, Some(&observed), String::new(), &mut out);
    if out.is_empty() { Ok(()) } else { Err(out) }
}

/// Escape a JSON object key as an RFC 6901 pointer token: `~` becomes `~0`
/// and `/` becomes `~1`, applied in that order so a key already containing a
/// literal `~0` or `~1` round-trips correctly. Array indices need no
/// escaping — they are always ASCII digits.
fn escape_pointer_token(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// Descend `planned`, comparing each leaf against the same position in
/// `observed`. Containers are walked; only leaves are compared *within* a
/// matching pair of containers — an object gaining a key is not itself a
/// violation. A container itself is compared structurally before descending:
/// if `observed` at this path is absent, or is not the same container kind
/// (object vs. object, array vs. array), that is reported once at the
/// container's own path and its children are not visited. Without this
/// check, an empty `planned` object or array is iterated zero times and
/// compared to nothing — so a dropped, mistyped or object-vs-scalar
/// container was silently invisible to this function.
fn walk(planned: &Value, observed: Option<&Value>, path: String, out: &mut Vec<Inconsistency>) {
    match planned {
        Value::Object(map) => {
            if !matches!(observed, Some(Value::Object(_))) {
                out.push(Inconsistency {
                    path,
                    planned: planned.clone(),
                    observed: observed.cloned(),
                    parse_failure: false,
                });
                return;
            }
            for (key, child) in map {
                let child_path = format!("{path}/{}", escape_pointer_token(key));
                let child_observed = observed.and_then(|o| o.get(key));
                walk(child, child_observed, child_path, out);
            }
        }
        Value::Array(items) => {
            if !matches!(observed, Some(Value::Array(_))) {
                out.push(Inconsistency {
                    path,
                    planned: planned.clone(),
                    observed: observed.cloned(),
                    parse_failure: false,
                });
                return;
            }
            for (i, child) in items.iter().enumerate() {
                let child_path = format!("{path}/{i}");
                let child_observed = observed.and_then(|o| o.get(i));
                walk(child, child_observed, child_path, out);
            }
        }
        leaf => {
            if observed != Some(leaf) {
                out.push(Inconsistency {
                    path,
                    planned: leaf.clone(),
                    observed: observed.cloned(),
                    parse_failure: false,
                });
            }
        }
    }
}

/// What an addon's `plan` produced, flattened for testing.
///
/// The WIT `plan-outcome` variant lives in the addon's own crate, generated by
/// `cargo component`; this crate cannot name it. An addon adapts its bindgen
/// type into this in one line, and `None` from the closure stands for
/// `deferred`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanResult {
    pub planned_json: String,
    pub requires_replace: Vec<String>,
}

/// Assert that planning a state against itself is a no-op.
///
/// This is the single most valuable property an addon can be held to. An
/// addon that fails it never converges: every reconcile finds work, the
/// resource never settles, and the symptom — a plan that is never clean —
/// looks like a platform fault rather than an addon defect.
///
/// # Errors
///
/// Returns a human-readable description of the first property violated:
/// a deferred outcome, a non-empty `requires_replace`, or a planned state
/// that differs from the input.
pub fn assert_plan_idempotent(
    current: &str,
    plan: impl Fn(&str, &str) -> Option<PlanResult>,
) -> Result<(), String> {
    let Some(result) = plan(current, current) else {
        return Err(
            "plan(x, x) returned deferred; nothing is missing when current and desired agree"
                .to_string(),
        );
    };

    if !result.requires_replace.is_empty() {
        return Err(format!(
            "plan(x, x) returned requires-replace {:?}; a state that already matches must not be \
             destroyed and recreated",
            result.requires_replace
        ));
    }

    // Both directions, deliberately. `assert_apply_consistent` checks a
    // SUBSET — every leaf of its first argument present in its second — which
    // is right for apply (an addon may report more than it planned) and wrong
    // here. Idempotency needs equality: running it only as
    // `(current, planned)` would pass an addon that ADDS a field on every
    // plan, which is the churning case this property exists to catch.
    let removed = assert_apply_consistent(current, &result.planned_json).err();
    let added = assert_apply_consistent(&result.planned_json, current).err();

    let mut shown: Vec<String> = Vec::new();
    for d in removed.into_iter().flatten() {
        if d.parse_failure {
            // `removed` and `added` both parse `result.planned_json` — one
            // failure there is one fact, not two, so only this pass reports
            // it. The message itself already names which side failed.
            let msg = d.planned.as_str().unwrap_or("input did not parse as JSON");
            shown.push(msg.to_string());
            continue;
        }
        shown.push(format!("{} dropped (planned {})", d.path, d.planned));
    }
    for d in added.into_iter().flatten() {
        if d.parse_failure {
            continue;
        }
        shown.push(format!("{} added (plan says {})", d.path, d.planned));
    }

    if shown.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "plan(x, x) did not return x unchanged, so this addon never converges: {}",
            shown.join("; ")
        ))
    }
}

/// Assert that planning the same inputs twice produces the same output.
///
/// A plan that varies cannot be approved: what the user saw in the plan is not
/// what the apply will do.
///
/// # Errors
///
/// Returns a description of how the two calls differed.
pub fn assert_plan_stable(
    current: &str,
    desired: &str,
    plan: impl Fn(&str, &str) -> Option<PlanResult>,
) -> Result<(), String> {
    let first = plan(current, desired);
    let second = plan(current, desired);
    if first == second {
        Ok(())
    } else {
        Err(format!(
            "two identical plan calls differed:\n  first:  {first:?}\n  second: {second:?}"
        ))
    }
}
