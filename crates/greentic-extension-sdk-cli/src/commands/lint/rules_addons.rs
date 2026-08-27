//! `contributions.addons` lint rules.
//!
//! The secret rule is the one that earns its keep. Spec D16 says credentials
//! never appear in `desired_state_schema`, because a value `observe` cannot
//! read back diffs forever and no plan is ever clean. That is a design
//! decision a reviewer would have to remember; here it is a rule.

use std::path::Path;

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
/// object, at any depth reachable through `properties`, `items`,
/// `prefixItems`, `contains`, `$defs`, `definitions`, `patternProperties`,
/// `dependentSchemas`, `additionalProperties`, `unevaluatedProperties`,
/// `allOf`, `anyOf`, `oneOf`, `if`, `then`, `else`. `path` accumulates a
/// human-readable pointer through `properties`/`items` nesting
/// (`acl_users[].password`) and most schema-composition keywords add
/// nothing (`allOf`, `anyOf`, `oneOf`, `patternProperties`,
/// `dependentSchemas`, `if`, `then`, `else` all pass `path` through
/// unchanged, since they don't correspond to a position of their own in the
/// *data* shape). `$defs` and
/// `definitions` are the exception: a def has no data position at all, since
/// it is reachable only through a `$ref` this walk deliberately never
/// resolves. So instead of passing `path` through, they insert a
/// `$defs/<name>` (or `definitions/<name>`) marker, keeping the reported
/// path honest about being a definition rather than implying a data
/// position that may not exist.
///
/// Only names appearing as property keys are ever candidates: the keys of
/// `patternProperties` are regexes, not property names, and the keys of
/// `$defs`/`definitions` are def names, not property names, so neither is
/// ever passed to `on_property` - only their *values* are walked further.
/// Schema keywords themselves (the literal string `"properties"`, etc.) are
/// never treated as candidates because they never appear as a map key
/// *inside* a `properties` object in the shapes this walks. `enum` and
/// `const` are not in the keyword set walked here, so their values are
/// never descended into. `propertyNames` is also deliberately not walked:
/// its schema validates each property *name* as a string, never the object
/// itself, so a `properties` map placed inside it is schema-legal but dead -
/// it can never apply to any actual data an addon declares - and walking it
/// would only add noise, not signal. `dependentRequired` is not walked
/// either: its values are arrays of property name strings, never schemas,
/// so there is nowhere for a `properties` map to go. `not` is likewise
/// deliberately excluded - see the note at its former call site below for
/// why negation is different from every other composition keyword this
/// walk covers.
///
/// # Why unbounded recursion is safe here
///
/// This function recurses with no explicit depth guard. That is safe today,
/// but only because of two facts that hold nowhere else in this file and
/// must both keep holding:
///
/// 1. **`$ref` is deliberately never resolved.** `$ref` is not one of the
///    keywords walked above, so a `$defs`/`$ref` pair is an ordinary finite
///    tree: the `$defs` value is walked once, directly, and a sibling `$ref`
///    pointing at it is never followed back down. Adding `$ref` resolution
///    would let a `$defs` entry's schema point at an ancestor of itself,
///    turning that finite tree into an actual cycle and this recursion into
///    an infinite one.
/// 2. **The input's nesting depth is bounded before it ever reaches this
///    function.** The only caller parses the schema with
///    `serde_json::from_str`, whose deserializer defaults to a
///    `remaining_depth` of 128 and errors out before producing a `Value` for
///    anything nested deeper. This crate never enables `serde_json`'s
///    `unbounded_depth` feature, so a schema nested past that limit fails to
///    *parse* - the caller's `if let Ok(parsed) = ...` simply skips it - and
///    never reaches this walk at all. See
///    `a_desired_state_schema_nested_past_serde_json_depth_limit_fails_to_parse`
///    in `tests.rs`, which pins this.
///
/// If either assumption stops holding - `$ref` resolution is added (extending
/// the set of keywords this walk covers is exactly when that temptation
/// shows up), or `unbounded_depth` is enabled anywhere in the dependency
/// graph - this function needs an explicit depth guard, because it would
/// then be walking attacker-controlled, effectively unbounded recursion at
/// publish/install time.
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

    // `prefixItems` is Draft 2020-12's replacement for tuple-form `items`
    // (array-form `items` is deprecated there): each entry is a schema for
    // the item at that index. Walked the same way as tuple-form `items`
    // above - one `[]` marker for every entry, since the reported path
    // doesn't try to track which index a property lives at.
    if let Some(serde_json::Value::Array(tuple)) = obj.get("prefixItems") {
        let child_path = format!("{path}[]");
        for item in tuple {
            walk_schema_properties(item, &child_path, on_property);
        }
    }

    // `contains` is a single schema an array item must match at least once -
    // the non-tuple counterpart of `items`. Walked the same way as
    // non-tuple `items`: one `[]` marker, since which item satisfies it is
    // unknown statically.
    if let Some(contains) = obj.get("contains")
        && contains.is_object()
    {
        let child_path = format!("{path}[]");
        walk_schema_properties(contains, &child_path, on_property);
    }

    // `$defs`/`definitions` keys are def names, not property names, so they
    // are never passed to `on_property` - but unlike every other keyword
    // walked here, a def is not inlined at `path`: it is only ever reached
    // through a `$ref` this walk deliberately never resolves (see the
    // "unbounded recursion" note above), so it has no data position at all.
    // Passing `path` through unchanged would report a violation at, e.g.,
    // `foo.password` when the def sits under `properties.foo.$defs` - a
    // position that does not exist in the addon's actual desired state and
    // would send the author looking in the wrong place. Insert a `$defs/
    // <name>` marker instead, so the reported path is honest about being a
    // definition rather than data.
    if let Some(map) = obj.get("$defs").and_then(|v| v.as_object()) {
        for (name, subschema) in map {
            let def_path = if path.is_empty() {
                format!("$defs/{name}")
            } else {
                format!("{path}.$defs/{name}")
            };
            walk_schema_properties(subschema, &def_path, on_property);
        }
    }
    if let Some(map) = obj.get("definitions").and_then(|v| v.as_object()) {
        for (name, subschema) in map {
            let def_path = if path.is_empty() {
                format!("definitions/{name}")
            } else {
                format!("{path}.definitions/{name}")
            };
            walk_schema_properties(subschema, &def_path, on_property);
        }
    }

    // `patternProperties` keys are regexes, not property names, so `path`
    // passes through unchanged and only the values are walked. Unlike
    // `$defs`/`definitions`, a `patternProperties` value schema *is* inlined
    // at the parent's data position - it is just that the specific matching
    // key is unknown - so no marker is needed here.
    if let Some(map) = obj.get("patternProperties").and_then(|v| v.as_object()) {
        for subschema in map.values() {
            walk_schema_properties(subschema, path, on_property);
        }
    }

    // `dependentSchemas` maps a property name to a schema that applies to
    // the *whole* object (not to that property's value) whenever the named
    // property is present. Its values are full schemas that can carry a
    // nested `properties` map at the same data position as the parent
    // object, so they are walked like `patternProperties`: path unchanged,
    // values only. The map's own keys are already real property names, but
    // every realistic use of `dependentSchemas` pairs a key with a
    // same-named entry under the object's own `properties` (the property's
    // type has to be declared somewhere), so that key is already a
    // candidate through the ordinary `properties` walk above - it is not
    // duplicated here.
    if let Some(map) = obj.get("dependentSchemas").and_then(|v| v.as_object()) {
        for subschema in map.values() {
            walk_schema_properties(subschema, path, on_property);
        }
    }

    if let Some(additional) = obj.get("additionalProperties")
        && additional.is_object()
    {
        walk_schema_properties(additional, path, on_property);
    }

    // `unevaluatedProperties` is Draft 2020-12's successor to
    // `additionalProperties` for properties left over after `allOf`/`if`/
    // `$ref` composition is accounted for. Like `additionalProperties` it
    // takes a schema (not only a boolean) and that schema applies at the
    // same, real leftover-property data position, so it is walked the same
    // way: path unchanged, only when it is an object.
    if let Some(unevaluated) = obj.get("unevaluatedProperties")
        && unevaluated.is_object()
    {
        walk_schema_properties(unevaluated, path, on_property);
    }

    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = obj.get(key).and_then(|v| v.as_array()) {
            for branch in branches {
                walk_schema_properties(branch, path, on_property);
            }
        }
    }

    // `if`, `then`, `else` are each a single schema constraining the *same*
    // data position as their parent, exactly like the `allOf`/`anyOf`/
    // `oneOf` branches above - applied conditionally, but that doesn't
    // change where a `properties` map inside them would land in the actual
    // data, so `path` passes through unchanged, same as
    // `additionalProperties`.
    //
    // `not` is deliberately NOT walked here, unlike every other
    // composition keyword above. `not: {"properties":{"admin_password":...}}`
    // means the instance must NOT have `admin_password` in that shape - the
    // author is forbidding the credential, not declaring one. Flagging a
    // name found only inside `not` would invert its meaning and punish an
    // author for writing the prohibition D16 recommends (the same mistake
    // this rule already made once, for `secret_ref`, and fixed). The bias
    // toward over-detection elsewhere in this file is deliberate; it does
    // not extend to a construct whose entire meaning is negation. Do not
    // re-add `not` to this list without re-reading this comment.
    for key in ["if", "then", "else"] {
        if let Some(sub) = obj.get(key)
            && sub.is_object()
        {
            walk_schema_properties(sub, path, on_property);
        }
    }
}

/// Strips `//` line comments and `/* */` block comments from a `.wit`
/// source, leaving everything else (including newlines, so statement
/// boundaries are unaffected) untouched.
///
/// WIT has no string literals in the shape `world.wit` files take here
/// (versions are written bare, `@0.1.0`, never quoted), so there is no
/// "comment marker inside a string" case to worry about — every `//` and
/// `/*` in a real `.wit` file starts an actual comment.
///
/// This exists because of a documented trap: the addon scaffold's own
/// `wit/world.wit.tmpl` carries an eight-line comment explaining *why* the
/// scaffold does NOT export `backup` — a comment that mentions
/// `addon-extension-with-backup` and `backup` by name while the world it
/// documents exports neither. A raw `contains("backup")` over the
/// unstripped file reports the exact opposite of the truth on the first
/// file anyone will test this rule against. Comments must be gone before
/// `world_exports_backup` ever looks at the text.
fn strip_wit_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'/') {
            for nc in chars.by_ref() {
                if nc == '\n' {
                    out.push('\n');
                    break;
                }
            }
            continue;
        }
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut prev = ' ';
            for nc in chars.by_ref() {
                if prev == '*' && nc == '/' {
                    break;
                }
                prev = nc;
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Whether a `.wit` world source exports the `backup` interface — either
/// qualified (`export greentic:extension-addon/backup@0.1.0;`, what the
/// addon scaffold and every real extension's `wit/world.wit` writes) or
/// bare (`export backup;`, valid only inside `extension-addon.wit` itself,
/// where `backup` is a sibling interface rather than an import). Comments
/// are stripped first — see `strip_wit_comments`.
///
/// Deliberately a statement scan, not a real WIT parse: `wit_parser` is a
/// dev-dependency only (used by
/// `crates/greentic-extension-sdk-cli/tests/wit_addon_parses.rs`), not
/// available to this binary, and even if it were, a lone `wit/world.wit`
/// that `use`s or exports `greentic:extension-addon/*` needs its
/// `wit/deps/` tree to resolve those references — `gtdx lint --dir` is
/// pointed at packed and installed extensions too, and nothing guarantees
/// `wit/deps/` survives packing. Full resolution would make lint unusable
/// on exactly the installs it needs to cover. Every `export`/`import`
/// statement in a world block is `<path>[@<version>];` with no nested `;`
/// of its own, so splitting the comment-stripped source on `;` and matching
/// the `export` statements is sufficient for the shape this contract's
/// world files actually take.
fn world_exports_backup(source: &str) -> bool {
    let stripped = strip_wit_comments(source);
    stripped.split(';').any(|stmt| {
        let Some(rest) = stmt.trim().strip_prefix("export") else {
            return false;
        };
        let Some(target) = rest.split_whitespace().next() else {
            return false;
        };
        let path_no_version = target.split('@').next().unwrap_or(target);
        path_no_version.rsplit('/').next() == Some("backup")
    })
}

/// Checks one addon's `supports_backup` claim against `backup_exported` (see
/// `world_exports_backup`; `None` means `wit/world.wit` was absent and this
/// is a no-op). Pushes `E_ADDON_BACKUP_NOT_EXPORTED` when the addon claims a
/// capability the world does not export. Returns whether this addon declared
/// `supports_backup: true`, which the caller aggregates across every addon
/// to decide `W_ADDON_BACKUP_UNDECLARED`. Split out of `check_addons` to
/// keep that function under clippy's line budget.
fn check_addon_backup_claim(
    addon: &serde_json::Value,
    id: &str,
    backup_exported: Option<bool>,
    out: &mut Vec<Violation>,
) -> bool {
    let supports_backup = addon
        .get("supports_backup")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if supports_backup && backup_exported == Some(false) {
        out.push(Violation::error(
            "E_ADDON_BACKUP_NOT_EXPORTED",
            format!(
                "addon {id:?} declares supports_backup: true, but wit/world.wit does not \
                 export greentic:extension-addon/backup. The platform will offer a pre-destroy \
                 snapshot for this addon and call an export that does not exist."
            ),
        ));
    }
    supports_backup
}

pub(super) fn check_addons(describe: &serde_json::Value, dir: &Path) -> Vec<Violation> {
    let mut out = Vec::new();
    let Some(addons) = describe
        .get("contributions")
        .and_then(|c| c.get("addons"))
        .and_then(|a| a.as_array())
    else {
        return out;
    };

    // `wit/world.wit` is the extension's own world — not
    // `wit/deps/greentic/.../world.wit`, which holds copies of the
    // *imported* contract packages. `gtdx new --kind addon` writes the
    // former at the project root (`crates/greentic-extension-sdk-cli/src/
    // commands/new/mod.rs`, the `.join("world.wit")` under `target`, not
    // under `wit/deps`).
    //
    // A missing file is not a violation of anything: `gtdx lint --dir` runs
    // against packed and installed extensions too, where the source tree
    // (including `wit/`) is legitimately absent. `None` here means both
    // backup rules below stay completely silent, for every addon.
    let backup_exported = std::fs::read_to_string(dir.join("wit").join("world.wit"))
        .ok()
        .map(|source| world_exports_backup(&source));

    let mut any_declares_backup = false;

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

        if check_addon_backup_claim(addon, id, backup_exported, &mut out) {
            any_declares_backup = true;
        }
    }

    // Drift in the other direction: the component genuinely implements
    // `backup`, but no addon in the catalogue says so. Not a lie the way
    // the error case is - a snapshot the platform is never told about is
    // merely unused, not broken - so this is a warning, not an error.
    if backup_exported == Some(true) && !any_declares_backup {
        out.push(Violation::warning(
            "W_ADDON_BACKUP_UNDECLARED",
            "wit/world.wit exports greentic:extension-addon/backup, but no addon in \
             contributions.addons declares supports_backup: true. The capability is \
             implemented but never advertised, so the platform will never offer to use it."
                .to_string(),
        ));
    }

    out
}
