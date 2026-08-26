//! `permissions` accepted unknown keys, which is why `oauthProviders`
//! validated while being absent from the schema — and why a typo'd
//! permission key passed validation while granting nothing.

use greentic_extension_sdk_contract::schema::validate_describe_json;

/// `validate_describe_json` takes a parsed `&serde_json::Value`, not a string.
fn validate(describe_json: &str) -> Result<(), greentic_extension_sdk_contract::ContractError> {
    let value: serde_json::Value =
        serde_json::from_str(describe_json).expect("fixture is valid JSON");
    validate_describe_json(&value)
}

/// A minimal but complete v2 describe, with `permissions` filled in by the
/// caller. Adapted from the `VALID` fixture in `schema_v2_validate.rs` (the
/// field names here — `compat.min_designer_version`, `metadata.name`,
/// `runtime.components.*.sha256`/`world` — are what the schema and the
/// typed `Permissions`/`DescribeJson` structs actually require; kept inline
/// rather than loaded from a fixture file so a schema change that breaks the
/// shape shows up here as a compile-visible diff).
fn describe_with_permissions(permissions: &str) -> String {
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
            "id": "greentic.perm-test",
            "name": "Permission Test",
            "version": "0.1.0",
            "summary": "Fixture for permission schema validation.",
            "author": {{ "name": "Greentic" }},
            "license": "MIT"
          }},
          "engine": {{ "greenticDesigner": ">=1.2.0", "extRuntime": "^0.12.0" }},
          "capabilities": {{ "offered": [], "required": [] }},
          "runtime": {{
            "memoryLimitMB": 64,
            "permissions": {permissions},
            "components": {{
              "main": {{
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "world": "greentic:extension-design/design-extension"
              }}
            }}
          }},
          "contributions": {{}}
        }}"#
    )
}

#[test]
fn known_permission_keys_validate() {
    let d = describe_with_permissions(
        r#"{ "network": ["https://api.example.com/*"],
              "secrets": ["greentic://tenant/*"],
              "callExtensionKinds": ["DesignExtension"],
              "llmRoles": ["some_role"],
              "oauthProviders": ["hubspot"] }"#,
    );
    let result = validate(&d);
    assert!(
        result.is_ok(),
        "known permission keys must validate: {result:?}"
    );
}

#[test]
fn a_typod_permission_key_is_rejected() {
    // `netwrok`, not `network`. Before additionalProperties:false this
    // validated cleanly and granted nothing — the extension then failed at
    // runtime with a permission error that pointed nowhere useful.
    let d = describe_with_permissions(r#"{ "netwrok": ["https://api.example.com/*"] }"#);
    assert!(
        validate(&d).is_err(),
        "an unknown permission key must be rejected, not silently ignored"
    );
}

#[test]
fn empty_permissions_validate() {
    let d = describe_with_permissions("{}");
    let result = validate(&d);
    assert!(
        result.is_ok(),
        "empty permissions must validate: {result:?}"
    );
}
