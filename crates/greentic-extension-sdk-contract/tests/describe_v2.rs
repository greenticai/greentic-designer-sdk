use greentic_extension_sdk_contract::{ComponentId, DescribeJson, ExtensionKind};
use std::str::FromStr;

const AC_V2: &str = r##"{
  "apiVersion": "greentic.ai/v2",
  "kind": "DesignExtension",
  "compat": {
    "min_designer_version": ">=1.2.0",
    "min_runner_version": "^0.12.0",
    "contract_version": "1.2.0"
  },
  "metadata": {
    "id": "greentic.adaptive-cards",
    "name": "Adaptive Cards",
    "version": "1.10.0",
    "summary": "Design and validate Adaptive Cards v1.6",
    "author": { "name": "Greentic" },
    "license": "MIT"
  },
  "engine": {
    "greenticDesigner": ">=1.2.0",
    "extRuntime": "^1.2.0"
  },
  "capabilities": {
    "offered": [{ "id": "greentic:adaptive-cards/validate", "version": "1.0.0" }],
    "required": []
  },
  "runtime": {
    "memoryLimitMB": 64,
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] },
    "components": {
      "adaptive-card": {
        "oci_ref": "oci://ghcr.io/greenticai/components/component-adaptive-card:1.2.0",
        "sha256": "11111111111111111111111111111111111111111111111111111111111111aa",
        "world": "greentic:component/adaptive-card@1.2.0"
      }
    }
  },
  "contributions": {
    "nodeTypes": [
      {
        "type_id": "adaptive-card",
        "label": "Adaptive Card",
        "category": "visual",
        "icon": "🎴",
        "color": "#0d9488",
        "complexity": "complex",
        "config_schema": "{}",
        "output_ports": [{ "name": "default", "label": "Next" }],
        "runtime_ref": "adaptive-card"
      }
    ],
    "tools": [
      {
        "name": "validate_card",
        "export": "greentic:extension-design/validation.validate-content",
        "runtime_ref": "adaptive-card"
      }
    ]
  }
}"##;

#[test]
fn ac_v2_parses() {
    let d: DescribeJson = serde_json::from_str(AC_V2).unwrap();
    assert_eq!(d.kind, ExtensionKind::Design);
    assert_eq!(d.metadata.id, "greentic.adaptive-cards");
    assert_eq!(d.compat.contract_version.to_string(), "1.2.0");
    assert_eq!(d.runtime.components.len(), 1);
    let cid = ComponentId::from_str("adaptive-card").unwrap();
    let comp = d.runtime.components.get(&cid).unwrap();
    assert!(comp.oci_ref.is_some());
    let nt = &d.contributions.node_types[0];
    assert_eq!(nt.runtime_ref.as_ref(), Some(&cid));
}

#[test]
fn missing_components_rejected() {
    let bad = AC_V2.replace(
        "\"components\": {\n      \"adaptive-card\": {\n        \"oci_ref\": \"oci://ghcr.io/greenticai/components/component-adaptive-card:1.2.0\",\n        \"sha256\": \"11111111111111111111111111111111111111111111111111111111111111aa\",\n        \"world\": \"greentic:component/adaptive-card@1.2.0\"\n      }\n    }",
        "\"components\": {}",
    );
    let r: Result<DescribeJson, _> = serde_json::from_str(&bad);
    assert!(r.is_err(), "runtime.components must not be empty");
}

#[test]
fn missing_compat_rejected() {
    let bad = AC_V2.replace(
        "\"compat\": {\n    \"min_designer_version\": \">=1.2.0\",\n    \"min_runner_version\": \"^0.12.0\",\n    \"contract_version\": \"1.2.0\"\n  },\n  ",
        "",
    );
    let r: Result<DescribeJson, _> = serde_json::from_str(&bad);
    assert!(r.is_err());
}

#[test]
fn round_trips() {
    let d: DescribeJson = serde_json::from_str(AC_V2).unwrap();
    let s = serde_json::to_string(&d).unwrap();
    let back: DescribeJson = serde_json::from_str(&s).unwrap();
    assert_eq!(back.metadata.id, d.metadata.id);
    assert_eq!(back.runtime.components.len(), 1);
}

#[test]
fn unknown_runtime_ref_in_node_type_rejected() {
    let bad = AC_V2.replace(
        "\"runtime_ref\": \"adaptive-card\"\n      }\n    ],\n    \"tools\"",
        "\"runtime_ref\": \"does-not-exist\"\n      }\n    ],\n    \"tools\"",
    );
    let r: Result<DescribeJson, _> = serde_json::from_str(&bad);
    assert!(
        r.is_err(),
        "runtime_ref must reference a key in runtime.components"
    );
}
