//! `contributions.addons` lint rules.
//!
//! The secret rule is the one that earns its keep. Spec D16 says credentials
//! never appear in `desired_state_schema`, because a value `observe` cannot
//! read back diffs forever and no plan is ever clean. That is a design
//! decision a reviewer would have to remember; here it is a rule.

use super::Violation;

/// Families this SDK version knows. Unknown ones warn rather than fail: the
/// list lives in a released binary while `describe.json` is signed and
/// immutable, so a hard error here would reject an addon that a newer
/// platform understands perfectly well.
const KNOWN_FAMILIES: [&str; 6] = [
    "vector-db",
    "cache",
    "sql",
    "queue",
    "object-store",
    "search",
];

/// Property names that name a credential. Matched case-insensitively against
/// the name with `-` and `_` stripped, so `api_key`, `apiKey` and `api-key`
/// all hit the same entry. Deliberately biased toward over-detection: a
/// false positive here is loud and self-explanatory (the author sees the
/// property named and renames it), while a false negative means an addon
/// that diffs forever and never converges, discovered much later. `token`
/// is handled separately below - see `looks_like_a_secret`.
const SECRET_MARKERS: [&str; 5] = ["password", "secret", "apikey", "credential", "passwd"];

/// Final segments (the head noun) that make a property name benign even
/// though an earlier segment contains a marker word. `password_policy` is a
/// policy *about* passwords, not a password; `secret_ref` is a reference to
/// where a secret lives, which is the shape spec D16 recommends *instead of*
/// the secret itself. Same head-noun trick as the `token` check below,
/// generalised to every marker.
const BENIGN_HEAD_NOUNS: [&str; 14] = [
    "ref",
    "name",
    "id",
    "policy",
    "length",
    "days",
    "iterations",
    "encryption",
    "backend",
    "limit",
    "rotation",
    "count",
    "enabled",
    "required",
];

/// First segments that turn a credential noun into a policy question about
/// it, rather than the credential's value: `require_password` asks "is a
/// password required", `allow_credentials` asks "are credentials allowed"
/// (the CORS-header sense). Mirrors `BENIGN_HEAD_NOUNS` from the other end
/// of the name - both exist because a property can name a credential concept
/// without holding a credential value.
const PREDICATE_PREFIXES: [&str; 2] = ["require", "allow"];

fn is_valid_addon_id(id: &str) -> bool {
    !id.is_empty()
        && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Output names become environment variables on the consuming service, so
/// they must survive that translation unchanged.
fn is_env_var_safe(name: &str) -> bool {
    !name.is_empty()
        && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Splits `property` into segments on `-`, `_`, and camelCase boundaries (a
/// lowercase-to-uppercase transition). Shared by `last_segment` and the
/// head-noun exemptions in `looks_like_a_secret`.
fn segments(property: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for c in property.chars() {
        if c == '-' || c == '_' {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
            prev_lower = false;
            continue;
        }
        if c.is_ascii_uppercase() && prev_lower && !current.is_empty() {
            segments.push(std::mem::take(&mut current));
        }
        current.push(c);
        prev_lower = c.is_ascii_lowercase();
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// The last segment of `property` - the head noun. Used by the `token`
/// check in `looks_like_a_secret`: unlike the other markers, `token` is
/// matched only when it names the *final* segment, not a modifier earlier
/// in the name. That is what tells apart `auth_token` (a token, held in the
/// `auth` slot) from `token_limit` (a limit, on tokens) and `max_tokens` (a
/// count, of tokens - "tokens" plural is a different segment than "token",
/// which is exactly why this can't be loosened back to a substring check).
fn last_segment(property: &str) -> String {
    segments(property).pop().unwrap_or_default()
}

fn looks_like_a_secret(property: &str) -> bool {
    let segs = segments(property);

    // A benign head noun (the final segment) means the property is a
    // policy/reference/count *about* a credential concept, not the
    // credential's value - `password_policy`, `secret_ref`, `api_key_id`.
    if let Some(last) = segs.last()
        && BENIGN_HEAD_NOUNS
            .iter()
            .any(|noun| last.eq_ignore_ascii_case(noun))
    {
        return false;
    }

    // A predicate-prefix first segment means the property asks a yes/no
    // question about the credential concept, not the credential's value -
    // `require_password`, `allow_credentials`.
    if let Some(first) = segs.first()
        && PREDICATE_PREFIXES
            .iter()
            .any(|prefix| first.eq_ignore_ascii_case(prefix))
    {
        return false;
    }

    let flat: String = property
        .chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect();
    if SECRET_MARKERS.iter().any(|m| flat.contains(m)) {
        return true;
    }
    last_segment(property).eq_ignore_ascii_case("token")
}

/// Recursively walks a JSON Schema value, calling `on_property` for every
/// name that appears as a **property key**: every key of a `properties`
/// object, at any depth reachable through `properties`, `items`, `$defs`,
/// `definitions`, `patternProperties`, `additionalProperties`, `allOf`,
/// `anyOf`, `oneOf`. `path` accumulates a human-readable pointer through
/// `properties`/`items` nesting only (`acl_users[].password`), not through
/// schema-composition keywords (`$defs`, `allOf`, ...), since those don't
/// correspond to a position in the *data* shape.
///
/// Only names appearing as property keys are ever candidates: the keys of
/// `patternProperties` are regexes, not property names, and the keys of
/// `$defs`/`definitions` are def names, not property names, so neither is
/// ever passed to `on_property` - only their *values* are walked further.
/// Schema keywords themselves (the literal string `"properties"`, etc.) are
/// never treated as candidates because they never appear as a map key
/// *inside* a `properties` object in the shapes this walks. `enum` and
/// `const` are not in the keyword set walked here, so their values are
/// never descended into.
fn walk_schema_properties(
    schema: &serde_json::Value,
    path: &str,
    on_property: &mut impl FnMut(&str, &str),
) {
    let Some(obj) = schema.as_object() else {
        return;
    };

    if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
        for (name, subschema) in props {
            let child_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            on_property(name, &child_path);
            walk_schema_properties(subschema, &child_path, on_property);
        }
    }

    if let Some(items) = obj.get("items") {
        let child_path = format!("{path}[]");
        match items {
            serde_json::Value::Array(tuple) => {
                for item in tuple {
                    walk_schema_properties(item, &child_path, on_property);
                }
            }
            _ => walk_schema_properties(items, &child_path, on_property),
        }
    }

    // `$defs`/`definitions` keys are def names, and `patternProperties` keys
    // are regexes - neither names a property, so `path` passes through
    // unchanged and only the values are walked.
    for key in ["$defs", "definitions", "patternProperties"] {
        if let Some(map) = obj.get(key).and_then(|v| v.as_object()) {
            for subschema in map.values() {
                walk_schema_properties(subschema, path, on_property);
            }
        }
    }

    if let Some(additional) = obj.get("additionalProperties")
        && additional.is_object()
    {
        walk_schema_properties(additional, path, on_property);
    }

    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = obj.get(key).and_then(|v| v.as_array()) {
            for branch in branches {
                walk_schema_properties(branch, path, on_property);
            }
        }
    }
}

pub(super) fn check_addons(describe: &serde_json::Value) -> Vec<Violation> {
    let mut out = Vec::new();
    let Some(addons) = describe
        .get("contributions")
        .and_then(|c| c.get("addons"))
        .and_then(|a| a.as_array())
    else {
        return out;
    };

    for addon in addons {
        let id = addon.get("id").and_then(|v| v.as_str()).unwrap_or_default();

        if !is_valid_addon_id(id) {
            out.push(Violation::error(
                "E_ADDON_ID_PATTERN",
                format!(
                    "addon id {id:?} must match ^[a-z0-9][a-z0-9-]*$ - it becomes part of \
                     `<extension_id>/<id>` on the platform"
                ),
            ));
        }

        let family = addon
            .get("family")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if !family.is_empty() && !KNOWN_FAMILIES.contains(&family) {
            out.push(Violation::warning(
                "W_ADDON_FAMILY_UNKNOWN",
                format!(
                    "addon {id:?} declares family {family:?}, which this SDK does not know \
                     (known: {}). A flow asking for a family will not match it unless the \
                     platform knows it too.",
                    KNOWN_FAMILIES.join(", ")
                ),
            ));
        }

        if let Some(outputs) = addon.get("outputs").and_then(|v| v.as_array()) {
            for out_spec in outputs {
                let name = out_spec
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if !is_env_var_safe(name) {
                    out.push(Violation::error(
                        "E_ADDON_OUTPUT_NAME",
                        format!(
                            "addon {id:?} output {name:?} must match ^[A-Za-z_][A-Za-z0-9_]*$ - \
                             outputs are injected as environment variables"
                        ),
                    ));
                }
            }
        }

        // D16: credentials reach the addon through its binding, never through
        // desired state. `config_schema` is deliberately not checked - config
        // is not reconciled against observed state, so it does not diff.
        let desired = addon
            .get("desired_state_schema")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(desired) {
            walk_schema_properties(&parsed, "", &mut |property, path| {
                if looks_like_a_secret(property) {
                    out.push(Violation::error(
                        "E_ADDON_SECRET_IN_DESIRED_STATE",
                        format!(
                            "addon {id:?} declares {path:?} in desired_state_schema. \
                             A credential there can never be read back by `observe`, so it \
                             diffs forever and no plan is ever clean. Credentials reach the \
                             addon through its runtime binding instead."
                        ),
                    ));
                }
            });
        }
    }

    out
}
