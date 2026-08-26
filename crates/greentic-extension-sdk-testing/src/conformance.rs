//! Conformance checks an addon can run before deploying.
//!
//! The rule in [`assert_apply_consistent`] is the one the platform enforces in
//! production. It lives here so an author runs the same rule rather than a
//! test that resembles it.

use serde_json::Value;

/// One leaf where the applied state disagrees with the plan that was approved.
#[derive(Debug, Clone, PartialEq)]
pub struct Inconsistency {
    /// JSON Pointer to the offending leaf. Empty when the input did not parse.
    pub path: String,
    /// What the plan said.
    pub planned: Value,
    /// What the apply produced. `None` when the key is absent entirely.
    pub observed: Option<Value>,
}

/// Assert that an apply honoured the plan it was given.
///
/// Every leaf in `planned_json` must appear at the same JSON Pointer in
/// `observed_json` with an equal value. Extra keys in the observed state are
/// allowed: an addon may report a server-assigned id or a timestamp it could
/// not have planned.
///
/// Returns every violation rather than the first, because an author fixing
/// them one at a time learns the shape of the problem slowly.
///
/// # Errors
///
/// Returns the list of disagreeing leaves. A single entry with an empty `path`
/// means one of the two inputs did not parse as JSON — reported as a failure
/// rather than a pass, since silently accepting unparseable state is how a
/// check stops checking.
pub fn assert_apply_consistent(
    planned_json: &str,
    observed_json: &str,
) -> Result<(), Vec<Inconsistency>> {
    let planned: Value = serde_json::from_str(planned_json).map_err(|e| {
        vec![Inconsistency {
            path: String::new(),
            planned: Value::String(format!("planned state is not valid JSON: {e}")),
            observed: None,
        }]
    })?;
    let observed: Value = serde_json::from_str(observed_json).map_err(|e| {
        vec![Inconsistency {
            path: String::new(),
            planned: Value::String(format!("observed state is not valid JSON: {e}")),
            observed: None,
        }]
    })?;

    let mut out = Vec::new();
    walk(&planned, Some(&observed), String::new(), &mut out);
    if out.is_empty() { Ok(()) } else { Err(out) }
}

/// Descend `planned`, comparing each leaf against the same position in
/// `observed`. Containers are walked; only leaves are compared, so an object
/// gaining a key is not itself a violation.
fn walk(planned: &Value, observed: Option<&Value>, path: String, out: &mut Vec<Inconsistency>) {
    match planned {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = format!("{path}/{key}");
                let child_observed = observed.and_then(|o| o.get(key));
                walk(child, child_observed, child_path, out);
            }
        }
        Value::Array(items) => {
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
                });
            }
        }
    }
}
