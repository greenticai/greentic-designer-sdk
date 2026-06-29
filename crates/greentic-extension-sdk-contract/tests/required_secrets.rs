//! Tests for the top-level `requiredSecrets` field on `DescribeJson`.
//!
//! Covers:
//! - Deserialization with `requiredSecrets` present → correct field values.
//! - Serialization of `SecretKey` as a transparent string.
//! - Minimal describe WITHOUT `requiredSecrets` → empty Vec, field omitted on
//!   round-trip (`skip_serializing_if` = `Vec::is_empty`).

use greentic_extension_sdk_contract::DescribeJson;

/// A minimal but fully-valid v2 describe document. Reused across tests.
const MINIMAL_V2: &str = r#"{
  "apiVersion": "greentic.ai/v2",
  "kind": "DesignExtension",
  "compat": {
    "min_designer_version": ">=1.2.0",
    "min_runner_version": "^0.12.0",
    "contract_version": "1.2.0"
  },
  "metadata": {
    "id": "greentic.test-ext",
    "name": "Test Extension",
    "version": "0.1.0",
    "summary": "A test extension",
    "author": { "name": "Greentic" },
    "license": "MIT"
  },
  "capabilities": { "offered": [], "required": [] },
  "runtime": {
    "memoryLimitMB": 32,
    "permissions": {},
    "components": {
      "main": {
        "oci_ref": "ghcr.io/greentic/test:0.1.0",
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "world": "greentic:x/design-extension"
      }
    }
  },
  "contributions": {}
}"#;

/// `requiredSecrets` with one entry deserializes to `required_secrets.len() == 1`,
/// and `key` serializes as the transparent string `"tavily/api_key"`.
#[test]
fn required_secrets_deserialize_and_key_serializes_as_string() {
    let json = MINIMAL_V2.replace(
        r#""contributions": {}"#,
        r#""contributions": {},
  "requiredSecrets": [{"key": "tavily/api_key", "required": true}]"#,
    );

    let d: DescribeJson = serde_json::from_str(&json).expect("must parse with requiredSecrets");
    assert_eq!(
        d.required_secrets.len(),
        1,
        "should have one secret requirement"
    );

    let req = &d.required_secrets[0];
    assert!(req.required, "required must be true");

    // SecretKey round-trips as a transparent string
    let key_value = serde_json::to_value(&req.key).expect("key must serialize");
    assert_eq!(
        key_value,
        serde_json::json!("tavily/api_key"),
        "SecretKey must serialize as transparent string"
    );
}

/// A describe without `requiredSecrets` deserializes to an empty Vec and the
/// field is omitted from serialized output (byte-identical round-trip omission).
#[test]
fn without_required_secrets_empty_and_omitted_on_roundtrip() {
    let d: DescribeJson = serde_json::from_str(MINIMAL_V2).expect("minimal describe must parse");

    assert!(
        d.required_secrets.is_empty(),
        "required_secrets must be empty when field absent"
    );

    let serialized = serde_json::to_string(&d).expect("must serialize");
    assert!(
        !serialized.contains("requiredSecrets"),
        "requiredSecrets must not appear when Vec is empty; got: {serialized}"
    );
}
