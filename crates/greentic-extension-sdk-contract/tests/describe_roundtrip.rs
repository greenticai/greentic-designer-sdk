use greentic_extension_sdk_contract::DescribeJson;

fn hex64(c: char) -> String {
    std::iter::repeat_n(c, 64).collect()
}

fn base_metadata() -> serde_json::Value {
    serde_json::json!({
        "id": "greentic.provider.telegram",
        "name": "Telegram",
        "version": "0.1.0",
        "summary": "Telegram messaging provider",
        "author": { "name": "Greentic" },
        "license": "Apache-2.0"
    })
}

fn base_engine() -> serde_json::Value {
    serde_json::json!({ "greenticDesigner": "*", "extRuntime": "^0.1.0" })
}

fn base_capabilities() -> serde_json::Value {
    serde_json::json!({ "offered": [], "required": [] })
}

fn base_permissions() -> serde_json::Value {
    serde_json::json!({ "network": [], "secrets": [], "callExtensionKinds": [] })
}

#[test]
fn describe_with_kind_provider_and_gtpack_roundtrips() {
    let json = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "ProviderExtension",
        "metadata": base_metadata(),
        "engine": base_engine(),
        "capabilities": base_capabilities(),
        "runtime": {
            "component": "wasm/provider_telegram_ext.wasm",
            "memoryLimitMB": 64,
            "permissions": base_permissions(),
            "gtpack": {
                "file": "runtime/provider.gtpack",
                "sha256": hex64('b'),
                "pack_id": "greentic.provider.telegram",
                "component_version": "0.6.0"
            }
        },
        "contributions": {}
    });
    let describe: greentic_extension_sdk_contract::DescribeJson =
        serde_json::from_value(json).unwrap();
    assert_eq!(
        describe.kind,
        greentic_extension_sdk_contract::ExtensionKind::Provider
    );
    assert!(describe.runtime.gtpack.is_some());
    assert_eq!(
        describe.runtime.gtpack.as_ref().unwrap().pack_id,
        "greentic.provider.telegram"
    );
    // Re-serialize and re-parse to verify round-trip
    let v = serde_json::to_value(&describe).unwrap();
    let round: greentic_extension_sdk_contract::DescribeJson = serde_json::from_value(v).unwrap();
    assert_eq!(round.kind, describe.kind);
}

#[test]
fn describe_with_kind_provider_requires_gtpack() {
    let json = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "ProviderExtension",
        "metadata": base_metadata(),
        "engine": base_engine(),
        "capabilities": base_capabilities(),
        "runtime": {
            "component": "wasm/provider_telegram_ext.wasm",
            "memoryLimitMB": 64,
            "permissions": base_permissions()
        },
        "contributions": {}
    });
    let err = serde_json::from_value::<greentic_extension_sdk_contract::DescribeJson>(json)
        .unwrap_err()
        .to_string()
        .to_lowercase();
    assert!(
        err.contains("gtpack") || err.contains("provider"),
        "error should explain missing gtpack field; got: {err}"
    );
}

#[test]
fn describe_non_provider_rejects_gtpack() {
    let json = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "DesignExtension",
        "metadata": base_metadata(),
        "engine": base_engine(),
        "capabilities": base_capabilities(),
        "runtime": {
            "component": "wasm/something.wasm",
            "memoryLimitMB": 64,
            "permissions": base_permissions(),
            "gtpack": {
                "file": "runtime/provider.gtpack",
                "sha256": hex64('c'),
                "pack_id": "x",
                "component_version": "0.6.0"
            }
        },
        "contributions": {}
    });
    let err = serde_json::from_value::<greentic_extension_sdk_contract::DescribeJson>(json);
    assert!(err.is_err(), "non-provider kinds must reject gtpack field");
}

const AC_FIXTURE: &str = r#"{
  "apiVersion": "greentic.ai/v1",
  "kind": "DesignExtension",
  "metadata": {
    "id": "greentic.adaptive-cards",
    "name": "Adaptive Cards",
    "version": "1.6.0",
    "summary": "Design AdaptiveCards v1.6",
    "author": { "name": "Greentic" },
    "license": "MIT"
  },
  "engine": {
    "greenticDesigner": ">=0.6.0",
    "extRuntime": "^0.1.0"
  },
  "capabilities": {
    "offered": [{ "id": "greentic:adaptive-cards/validate", "version": "1.0.0" }],
    "required": [{ "id": "greentic:host/logging", "version": "^1.0.0" }]
  },
  "runtime": {
    "component": "extension.wasm",
    "memoryLimitMB": 64,
    "permissions": {}
  },
  "contributions": {}
}"#;

#[test]
fn ac_fixture_parses() {
    let d: DescribeJson = serde_json::from_str(AC_FIXTURE).unwrap();
    assert_eq!(d.metadata.id, "greentic.adaptive-cards");
    assert_eq!(d.identity_key(), "greentic.adaptive-cards@1.6.0");
    assert_eq!(d.capabilities.offered.len(), 1);
    assert_eq!(d.runtime.memory_limit_mb, 64);
}

#[test]
fn round_trips_without_data_loss() {
    let d: DescribeJson = serde_json::from_str(AC_FIXTURE).unwrap();
    let serialized = serde_json::to_string(&d).unwrap();
    let parsed_back: DescribeJson = serde_json::from_str(&serialized).unwrap();
    assert_eq!(parsed_back.metadata.id, d.metadata.id);
}

const BUNDLE_STANDARD_FIXTURE: &str = r#"{
  "apiVersion": "greentic.ai/v1",
  "kind": "BundleExtension",
  "metadata": {
    "id": "greentic.bundle-standard",
    "name": "Standard Bundle Recipe",
    "version": "0.1.0",
    "summary": "Package designer session into a Greentic pack (.gtpack ZIP)",
    "author": { "name": "Greentic" },
    "license": "MIT"
  },
  "engine": {
    "greenticDesigner": ">=0.6.0",
    "extRuntime": "^0.1.0"
  },
  "capabilities": {
    "offered": [{ "id": "greentic:bundle/standard", "version": "0.1.0" }],
    "required": []
  },
  "runtime": {
    "component": "extension.wasm",
    "memoryLimitMB": 128,
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
  },
  "execution": {
    "kind": "builtin",
    "builtinId": "standard"
  },
  "contributions": {
    "recipes": [
      {
        "id": "standard",
        "displayName": "Standard Greentic Pack",
        "description": "Package designer session into a .gtpack archive",
        "configSchema": "schemas/standard.config.schema.json"
      }
    ]
  }
}"#;

#[test]
fn bundle_extension_with_execution_parses() {
    let d: DescribeJson = serde_json::from_str(BUNDLE_STANDARD_FIXTURE).unwrap();
    assert_eq!(d.metadata.id, "greentic.bundle-standard");
    assert_eq!(
        d.kind,
        greentic_extension_sdk_contract::ExtensionKind::Bundle
    );
    let exec = d.execution.as_ref().expect("execution present");
    assert_eq!(exec["kind"], "builtin");
    assert_eq!(exec["builtinId"], "standard");
}

#[test]
fn bundle_extension_without_execution_also_parses() {
    // `execution` is optional at contract level — bundle-specific readers
    // enforce its presence for kind=Bundle. The unified contract just
    // shouldn't reject BundleExtension descriptors that omit it.
    let json = BUNDLE_STANDARD_FIXTURE.replace(
        "\"execution\": {\n    \"kind\": \"builtin\",\n    \"builtinId\": \"standard\"\n  },\n  ",
        "",
    );
    let d: DescribeJson = serde_json::from_str(&json).unwrap();
    assert!(d.execution.is_none());
    assert_eq!(
        d.kind,
        greentic_extension_sdk_contract::ExtensionKind::Bundle
    );
}

#[test]
fn non_bundle_rejects_execution() {
    let json = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "DesignExtension",
        "metadata": base_metadata(),
        "engine": base_engine(),
        "capabilities": base_capabilities(),
        "runtime": {
            "component": "extension.wasm",
            "memoryLimitMB": 64,
            "permissions": base_permissions()
        },
        "execution": { "kind": "builtin", "builtinId": "standard" },
        "contributions": {}
    });
    let err = serde_json::from_value::<DescribeJson>(json)
        .unwrap_err()
        .to_string()
        .to_lowercase();
    assert!(
        err.contains("execution") && err.contains("bundleextension"),
        "error should explain execution-on-non-bundle; got: {err}"
    );
}

#[test]
fn bundle_extension_roundtrips_execution() {
    let d: DescribeJson = serde_json::from_str(BUNDLE_STANDARD_FIXTURE).unwrap();
    let serialized = serde_json::to_string(&d).unwrap();
    let parsed_back: DescribeJson = serde_json::from_str(&serialized).unwrap();
    assert_eq!(parsed_back.execution, d.execution);
}
