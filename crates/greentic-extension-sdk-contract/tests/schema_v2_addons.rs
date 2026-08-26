//! The schema and the Rust struct must agree. The struct is
//! `deny_unknown_fields`, so a schema that is more permissive lets a
//! descriptor pass validation and then fail on load, blaming the wrong layer.
//!
//! The fixture shape here is derived from the known-good `VALID` constant in
//! `schema_v2_validate.rs` (also mirrored in `describe_addons_invariants.rs`),
//! varying only the `contributions` block. The brief that seeded this test
//! carried a hand-written fixture using `min_designer`/`min_runner`/`contract`
//! under `compat` and a `display_name` under `metadata`; the real fields are
//! `min_designer_version`/`min_runner_version`/`contract_version` and
//! `metadata` requires `id`, `name`, `version`, `summary`, `author`,
//! `license` (no `display_name`, `description` optional). This file uses the
//! verified names.

use greentic_extension_sdk_contract::schema::validate_describe_json;

fn validate(describe_json: &str) -> Result<(), greentic_extension_sdk_contract::ContractError> {
    let value: serde_json::Value =
        serde_json::from_str(describe_json).expect("fixture is valid JSON");
    validate_describe_json(&value)
}

/// Built from the known-good `VALID` fixture in `schema_v2_validate.rs`; only
/// `contributions` varies.
fn describe_with_addons(addons: &str) -> String {
    format!(
        r#"{{
          "$schema": "https://store.greentic.cloud/schemas/describe-v2.json",
          "apiVersion": "greentic.ai/v2",
          "kind": "DesignExtension",
          "compat": {{
            "min_designer_version": ">=1.2.0",
            "min_runner_version": "^0.12.0",
            "contract_version": "1.2.0"
          }},
          "metadata": {{
            "id": "greentic.addon-test",
            "name": "Addon Test",
            "version": "0.1.0",
            "summary": "Fixture.",
            "author": {{ "name": "Greentic" }},
            "license": "MIT"
          }},
          "capabilities": {{ "offered": [], "required": [] }},
          "runtime": {{
            "memoryLimitMB": 64,
            "permissions": {{ "network": [], "secrets": [], "callExtensionKinds": [] }},
            "components": {{
              "main": {{
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "world": "greentic:extension-design/design-extension",
                "gtpack": {{
                  "file": "extension.wasm",
                  "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                  "pack_id": "main"
                }}
              }}
            }}
          }},
          "contributions": {{ "addons": {addons} }}
        }}"#
    )
}

#[test]
fn a_full_addon_validates() {
    let d = describe_with_addons(
        r#"[{
            "id": "qdrant",
            "family": "vector-db",
            "display_name": "Qdrant",
            "description": "Vector database.",
            "icon": "database",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}",
            "outputs": [{ "name": "url", "type": "text", "sensitive": false }],
            "supports_backup": true,
            "schema_version": 2
        }]"#,
    );
    let r = validate(&d);
    assert!(r.is_ok(), "a full addon must validate: {r:?}");
}

#[test]
fn a_minimal_addon_validates() {
    let d = describe_with_addons(
        r#"[{
            "id": "redis",
            "family": "cache",
            "display_name": "Redis",
            "description": "Key-value store.",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}"
        }]"#,
    );
    let r = validate(&d);
    assert!(r.is_ok(), "optional fields may be omitted: {r:?}");
}

#[test]
fn an_addon_missing_a_required_field_is_rejected() {
    // No `family`.
    let d = describe_with_addons(
        r#"[{
            "id": "redis",
            "display_name": "Redis",
            "description": "Key-value store.",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}"
        }]"#,
    );
    assert!(validate(&d).is_err(), "family is required");
}

/// The struct is `deny_unknown_fields`; the schema must be too, or a typo
/// passes validation and fails on load.
#[test]
fn an_unknown_addon_field_is_rejected() {
    let d = describe_with_addons(
        r#"[{
            "id": "redis",
            "family": "cache",
            "display_name": "Redis",
            "description": "Key-value store.",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}",
            "supports_backups": true
        }]"#,
    );
    assert!(
        validate(&d).is_err(),
        "an unknown addon field must be rejected"
    );
}

/// The schema is the layer that gates `gtdx publish`, install, signature
/// verification and OCI pull — `gtdx lint` is opt-in and nothing invokes it
/// automatically. An id the lint rejects (`E_ADDON_ID_PATTERN`) must be
/// rejected here too, or it sails through every enforced call site and only
/// becomes ambiguous once namespaced to `<extension_id>/<id>`.
#[test]
fn an_addon_id_that_the_lint_rejects_is_also_rejected_by_the_schema() {
    let d = describe_with_addons(
        r#"[{
            "id": "Qdrant/Primary",
            "family": "vector-db",
            "display_name": "Qdrant",
            "description": "Vector database.",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}"
        }]"#,
    );
    assert!(
        validate(&d).is_err(),
        "an id containing '/' and uppercase must be rejected by the schema"
    );
}

/// Same dual-enforcement requirement as the id, for `outputs[].name`.
#[test]
fn an_addon_output_name_that_the_lint_rejects_is_also_rejected_by_the_schema() {
    let d = describe_with_addons(
        r#"[{
            "id": "qdrant",
            "family": "vector-db",
            "display_name": "Qdrant",
            "description": "Vector database.",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}",
            "outputs": [{ "name": "qdrant-url", "type": "text" }]
        }]"#,
    );
    assert!(
        validate(&d).is_err(),
        "an output name with a hyphen must be rejected by the schema"
    );
}

#[test]
fn an_unknown_output_type_is_rejected() {
    let d = describe_with_addons(
        r#"[{
            "id": "redis",
            "family": "cache",
            "display_name": "Redis",
            "description": "Key-value store.",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}",
            "outputs": [{ "name": "url", "type": "object" }]
        }]"#,
    );
    assert!(
        validate(&d).is_err(),
        "output type must be text|number|boolean"
    );
}
