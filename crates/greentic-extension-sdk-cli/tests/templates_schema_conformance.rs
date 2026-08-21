//! Every `gtdx new` describe.json template must validate against the
//! describe-v2 JSON Schema the contract crate ships (and the store server
//! mirrors). This is the regression guard for the SDK→store conformance
//! audit: a template that drifts from the schema — a renamed key, a new
//! field the schema rejects, a wrong `apiVersion` — fails here at `cargo
//! test` instead of at `gtdx publish` against a live store.
//!
//! The templates are rendered with placeholder values mirroring
//! `commands::new`'s substitution map; an unknown `{{placeholder}}` fails
//! the test loudly so a newly-introduced template variable can't slip
//! through unvalidated.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use greentic_extension_sdk_contract::schema::validate_describe_v2;

fn templates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")
}

/// Placeholder → value, mirroring the substitution `commands::new` performs.
/// Values are chosen to be schema-valid (semver versions, reverse-DNS id).
fn substitutions() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("name", "my-ext"),
        ("name_cargo", "my-ext"),
        ("kind", "design"),
        ("id", "com.example.my-ext"),
        ("id_wit", "com-example:my-ext"),
        ("version", "0.1.0"),
        ("author", "Jane Doe"),
        ("license", "Apache-2.0"),
        ("sdk_version", "1.2.7-research"),
        (
            "min_designer_version",
            greentic_extension_sdk_contract::compat::MIN_DESIGNER_VERSION,
        ),
        ("runtime_ref_key", "my-ext"),
        ("node_type_id", "my-ext"),
        ("label", "My Ext"),
        ("contract_version", "0.1.0"),
        // `wasm-component`-only: the OCI reference of the component that
        // executes the contributed node. Digest-pinned, because that is the
        // shape the scaffold emits and the one the docs insist on.
        (
            "component_ref",
            "oci://ghcr.io/greenticai/component/component-my-ext@sha256:461c6a68db12b1148465010589c7a8447bf5da9b9de358e4ae0758178801b959",
        ),
        // `openapi-connector`-only placeholders: `network_json`/`secrets_json`/
        // `tools_contrib_json` are substituted as raw JSON (no surrounding
        // quotes in the template), so their values must themselves be valid
        // JSON array literals.
        ("summary", "Generated connector for an OpenAPI spec."),
        ("network_json", "[\"https://api.example.com/*\"]"),
        ("secrets_json", "[\"secret://my-ext/bearerAuth\"]"),
        (
            "tools_contrib_json",
            "[{\"name\":\"getPetById\",\"export\":\"greentic:extension-design/tools.invoke-tool\",\"runtime_ref\":\"my-ext\"}]",
        ),
    ])
}

/// Replace every `{{key}}` with its mapped value. Panics on an unmapped
/// placeholder so the test fails the moment a template adds a new variable
/// the harness doesn't know how to fill (rather than emitting invalid JSON).
fn render(template: &str, subs: &BTreeMap<&'static str, &'static str>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .unwrap_or_else(|| panic!("unterminated placeholder in template"));
        let key = after[..end].trim();
        let value = subs
            .get(key)
            .unwrap_or_else(|| panic!("template uses unmapped placeholder {{{{{key}}}}}"));
        out.push_str(value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn template_files() -> Vec<PathBuf> {
    let dir = templates_dir();
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read templates dir {}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let describe = path.join("describe.json.tmpl");
            describe.is_file().then_some(describe)
        })
        .collect();
    found.sort();
    found
}

#[test]
fn every_new_template_describe_validates_against_describe_v2() {
    let subs = substitutions();
    let files = template_files();
    assert!(
        !files.is_empty(),
        "no describe.json.tmpl found under {}",
        templates_dir().display()
    );

    for file in &files {
        let kind_dir = file
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>");
        let raw = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        let rendered = render(&raw, &subs);
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap_or_else(|e| {
            panic!("template '{kind_dir}' rendered invalid JSON: {e}\n{rendered}")
        });

        // `mcp` scaffolds a `wasix:mcp/router` component, not a greentic v2
        // design extension. Its describe.json carries `kind:
        // "wasix:mcp/router"`, which is outside the greentic `describe-v2`
        // schema's `kind` enum by design — so it is validated separately here,
        // not against the v2 schema.
        if kind_dir == "mcp" {
            assert_eq!(
                value.get("kind").and_then(|v| v.as_str()),
                Some("wasix:mcp/router"),
                "mcp template must declare kind wasix:mcp/router"
            );
            continue;
        }

        validate_describe_v2(&value).unwrap_or_else(|e| {
            panic!("template '{kind_dir}' describe.json fails describe-v2 schema: {e}")
        });
    }
}

#[test]
fn wasm_component_node_uses_canonical_outcome_ports() {
    // A scaffolded node's output ports must use the canonical runtime outcome
    // vocabulary (`on_success` / `on_error`), matching greentic-types'
    // `ComponentDescribe.outcomes` convention and the flow-builder catalog's
    // event names — not bare `success` / `error`, which would mismatch routing.
    let subs = substitutions();
    let file = templates_dir()
        .join("wasm-component")
        .join("describe.json.tmpl");
    let raw =
        std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
    let value: serde_json::Value = serde_json::from_str(&render(&raw, &subs))
        .unwrap_or_else(|e| panic!("wasm-component rendered invalid JSON: {e}"));
    let names: Vec<&str> = value["contributions"]["nodeTypes"][0]["output_ports"]
        .as_array()
        .expect("wasm-component node must declare output_ports")
        .iter()
        .filter_map(|port| port["name"].as_str())
        .collect();
    assert_eq!(names, vec!["on_success", "on_error"]);
}

/// A scaffold must declare the *contract floor* it actually needs, not the
/// version of the SDK that generated it.
///
/// Templates used to pin `min_designer_version` (and the v1-equivalent
/// `engine.greenticDesigner`) to `>={{sdk_version}}`. Those are different
/// axes: an extension generated by SDK 1.3.x runs fine on any designer that
/// understands the v2 describe contract, i.e. 1.2.0 and up. Pinning to the
/// SDK version makes every freshly scaffolded extension declare itself
/// incompatible with perfectly capable designers — which `gtdx doctor` then
/// faithfully reports as a spurious "designer too old".
#[test]
fn templates_declare_the_contract_floor_not_the_sdk_version() {
    let subs = substitutions();
    let expected = format!(
        ">={}",
        greentic_extension_sdk_contract::compat::MIN_DESIGNER_VERSION
    );
    let expected = expected.as_str();

    for file in &template_files() {
        let kind_dir = file
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>");
        let raw = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        let value: serde_json::Value = serde_json::from_str(&render(&raw, &subs))
            .unwrap_or_else(|e| panic!("template '{kind_dir}' rendered invalid JSON: {e}"));

        if let Some(declared) = value.pointer("/compat/min_designer_version") {
            assert_eq!(
                declared.as_str(),
                Some(expected),
                "template '{kind_dir}' compat.min_designer_version"
            );
        }
        if let Some(declared) = value.pointer("/engine/greenticDesigner") {
            assert_eq!(
                declared.as_str(),
                Some(expected),
                "template '{kind_dir}' engine.greenticDesigner"
            );
        }
    }
}
