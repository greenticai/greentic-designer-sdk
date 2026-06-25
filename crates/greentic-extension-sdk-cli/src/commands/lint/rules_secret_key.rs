//! Lint rule: `E_SECRET_KEY_NOT_CANONICAL` (S3 / D2)
//!
//! The canonical secret-key format (decision D2) is `namespace/name`-style:
//! lowercase ASCII `[a-z0-9._-/]` only, non-empty, no leading or trailing `/`,
//! no `..` segment, no `://` substring.
//!
//! Keys such as `SLACK_BOT_TOKEN` (uppercase) or `*` (wildcard) are
//! **non-canonical** and should be renamed by the author before publish.
//! Auto-transforming them would break secret resolution because the key IS the
//! resolution identifier (`secret://<key>`). We warn but do not transform.

use super::Violation;

// ---------------------------------------------------------------------------
// Canonical-key predicate (inline — no greentic-types dep to avoid the
// release-train cap)
// ---------------------------------------------------------------------------

/// Returns `true` when `key` satisfies the D2 canonical form:
///
/// - Non-empty
/// - Every character in `[a-z0-9._\-/]` (lowercase only; uppercase rejected)
/// - Does not start or end with `/`
/// - Contains no `..` segment (any `..` path component)
/// - Contains no `://` substring (rules out URI-style grant strings)
fn is_canonical(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    // Reject URI-style grants (e.g. "secret://tavily/api_key")
    if key.contains("://") {
        return false;
    }
    // Reject wildcard and other non-path tokens
    if !key
        .chars()
        .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '.' | '_' | '-' | '/'))
    {
        return false;
    }
    // Leading or trailing '/'
    if key.starts_with('/') || key.ends_with('/') {
        return false;
    }
    // No ".." segment anywhere
    if key.split('/').any(|seg| seg == "..") {
        return false;
    }
    true
}

// ---------------------------------------------------------------------------
// Key extraction helpers
// ---------------------------------------------------------------------------

/// Collect every `key` string from a JSON array of `{key: …}` objects.
fn keys_from_array(arr: &[serde_json::Value]) -> impl Iterator<Item = &str> {
    arr.iter()
        .filter_map(|item| item.get("key").and_then(|v| v.as_str()))
}

// ---------------------------------------------------------------------------
// Rule
// ---------------------------------------------------------------------------

/// `E_SECRET_KEY_NOT_CANONICAL` — errors for each declared secret key that is
/// not in the canonical `namespace/name` lowercase D2 form.
///
/// Scanned locations:
/// - Top-level `requiredSecrets[].key`
/// - `contributions.tools[].secret_requirements[].key`
pub(super) fn check_secret_key_canonical(describe: &serde_json::Value) -> Vec<Violation> {
    let mut out = Vec::new();

    // 1. Top-level requiredSecrets
    if let Some(arr) = describe.get("requiredSecrets").and_then(|v| v.as_array()) {
        for key in keys_from_array(arr) {
            if !is_canonical(key) {
                out.push(make_violation(key));
            }
        }
    }

    // 2. contributions.tools[].secret_requirements[].key
    if let Some(tools) = describe
        .pointer("/contributions/tools")
        .and_then(|v| v.as_array())
    {
        for tool in tools {
            if let Some(reqs) = tool.get("secret_requirements").and_then(|v| v.as_array()) {
                for key in keys_from_array(reqs) {
                    if !is_canonical(key) {
                        out.push(make_violation(key));
                    }
                }
            }
        }
    }

    out
}

fn make_violation(key: &str) -> Violation {
    Violation::error(
        "E_SECRET_KEY_NOT_CANONICAL",
        format!(
            "secret key {key:?} is not in canonical form; \
             rename to lowercase `namespace/name` style using only [a-z0-9._-/] \
             (no leading/trailing `/`, no `..` segment, no `://`). \
             Non-canonical keys break `secret://<key>` resolution."
        ),
    )
}
