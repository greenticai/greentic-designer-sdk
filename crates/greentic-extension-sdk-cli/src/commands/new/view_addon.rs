//! Post-render describe authoring for `gtdx new --with-view`.
//!
//! The view is patched into the rendered `describe.json` rather than shipped
//! as a template overlay because `overlay()` replaces whole files: a
//! `view-addon/describe.json.tmpl` would have to duplicate every kind's
//! describe template and would drift from all of them. `commands::openapi`
//! already authors a describe this way.

use anyhow::Result;
use greentic_extension_sdk_contract::describe::UiPermissions;
use greentic_extension_sdk_contract::describe::contributions::{Placement, View};

use super::capabilities::ViewSpec;

/// The tool the example view is allowed to call, plus a placeholder argument
/// object shaped to satisfy that tool's own `input_schema`.
#[derive(Debug)]
pub(super) struct ChosenTool {
    pub name: String,
    /// A JSON object with one type-matched placeholder per field the tool's
    /// `input_schema` marks `required`. Empty (`{}`) when the tool declares
    /// no schema, no `required` list, or a schema this couldn't parse — a
    /// placeholder argument is a convenience for the example call, not a
    /// contract, so it never fails the scaffold.
    pub args: serde_json::Value,
}

/// Pick the tool the example view is allowed to call: the first entry in
/// `contributions.tools[]`, whatever that kind happens to contribute.
///
/// The deserializer invariant in `greentic-extension-sdk-contract` rejects any
/// `views[].tools` entry that doesn't name a tool the extension itself
/// contributes, so this must never hardcode a name — kinds differ (`design`
/// contributes `echo`, `llm` contributes `complete`, `bundle`/`deploy`/
/// `provider` contribute none at all). `None` means the kind contributes no
/// tools; the view still ships, it just can't call one yet.
fn first_contributed_tool(describe: &serde_json::Value) -> Option<ChosenTool> {
    let tool = describe
        .pointer("/contributions/tools")
        .and_then(|tools| tools.as_array())
        .and_then(|tools| tools.first())?;
    let name = tool.get("name")?.as_str()?.to_string();
    let args = tool
        .get("input_schema")
        .and_then(|schema| schema.as_str())
        .map_or_else(|| serde_json::json!({}), derive_placeholder_args);
    Some(ChosenTool { name, args })
}

/// Build a placeholder argument object that satisfies a tool's own JSON
/// Schema well enough to pass validation on the example view's first click:
/// one type-matched placeholder per field in `required`. A field's declared
/// `properties.<field>.type` selects the placeholder (`"hello"` for
/// `string`, `1` for `number`/`integer`, `true` for `boolean`, `[]` for
/// `array`, `{}` for `object`; anything else, including a missing type,
/// falls back to `"hello"`). No `input_schema`, no `required`, or a string
/// that doesn't parse as JSON all fall back to `{}` — this is scaffold
/// convenience, never a reason to fail `gtdx new`.
fn derive_placeholder_args(input_schema_json: &str) -> serde_json::Value {
    let schema: serde_json::Value = match serde_json::from_str(input_schema_json) {
        Ok(v) => v,
        Err(_) => return serde_json::json!({}),
    };
    let Some(required) = schema.get("required").and_then(|r| r.as_array()) else {
        return serde_json::json!({});
    };
    let properties = schema.get("properties").and_then(|p| p.as_object());

    let mut args = serde_json::Map::new();
    for field in required {
        let Some(field_name) = field.as_str() else {
            continue;
        };
        let field_type = properties
            .and_then(|props| props.get(field_name))
            .and_then(|prop| prop.get("type"))
            .and_then(|t| t.as_str());
        let placeholder = match field_type {
            Some("number" | "integer") => serde_json::json!(1),
            Some("boolean") => serde_json::json!(true),
            Some("array") => serde_json::json!([]),
            Some("object") => serde_json::json!({}),
            _ => serde_json::json!("hello"),
        };
        args.insert(field_name.to_string(), placeholder);
    }
    serde_json::Value::Object(args)
}

/// Insert the contributed view and its `permissions.ui` block into a rendered
/// describe. Returns the chosen tool (if any), so the caller can render the
/// example page's `{{view_tool}}` and `{{view_tool_args}}` placeholders to
/// match.
///
/// The view is built as the contract's own [`View`] and serialized, rather
/// than assembled as hand-written JSON: a renamed or added field on the
/// contract then shows up here as a compile error instead of as a describe the
/// designer silently refuses to parse.
///
/// # Errors
///
/// Fails when the rendered describe carries no `contributions` object or no
/// `runtime.permissions` block to attach the grants to.
pub(super) fn add_view_to_describe(
    describe: &mut serde_json::Value,
    spec: &ViewSpec,
) -> Result<Option<ChosenTool>> {
    let chosen_tool = first_contributed_tool(describe);

    let view = View {
        id: spec.id.clone(),
        surface: spec.surface,
        title_key: format!("view.{}.label", spec.id),
        title_fallback: spec.title_fallback.clone(),
        icon: None,
        entry: "index.html".to_string(),
        placement: Placement {
            slot: spec.slot.clone(),
            path: Vec::new(),
            order: None,
        },
        min_visibility: spec.min_visibility,
        tools: chosen_tool
            .as_ref()
            .map(|tool| vec![tool.name.clone()])
            .unwrap_or_default(),
    };

    let contributions = describe
        .get_mut("contributions")
        .and_then(|c| c.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("rendered describe.json has no contributions object"))?;
    contributions.insert(
        "views".to_string(),
        serde_json::Value::Array(vec![serde_json::to_value(&view)?]),
    );

    // `UiPermissions` skips both lists when empty, but the block itself is
    // written unconditionally: its presence is what tells an author where the
    // view's grants go, and an absent `ui` key reads as "this view only
    // renders" rather than "you have not filled this in yet".
    let ui = UiPermissions {
        fetch_hosts: spec.fetch_hosts.clone(),
        platform_api: spec.platform_api.clone(),
    };
    let permissions = describe
        .pointer_mut("/runtime/permissions")
        .and_then(|p| p.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("rendered describe.json has no runtime.permissions"))?;
    permissions.insert(
        "ui".to_string(),
        serde_json::json!({
            "fetchHosts": ui.fetch_hosts,
            "platformApi": serde_json::to_value(&ui.platform_api)?,
        }),
    );

    Ok(chosen_tool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_extension_sdk_contract::describe::ApiGrant;
    use greentic_extension_sdk_contract::describe::contributions::{Surface, Visibility};

    const RENDERED: &str = r#"{
  "contributions": { "tools": [{ "name": "echo" }] },
  "runtime": { "permissions": { "network": [] } }
}"#;

    /// The defaults `--with-view` alone produces.
    fn spec() -> ViewSpec {
        ViewSpec {
            id: "hello".to_string(),
            surface: Surface::Designer,
            slot: "designer.sidebar".to_string(),
            title_fallback: "Hello".to_string(),
            min_visibility: Visibility::Member,
            fetch_hosts: Vec::new(),
            platform_api: Vec::new(),
        }
    }

    fn patch(rendered: &str, spec: &ViewSpec) -> (serde_json::Value, Option<ChosenTool>) {
        let mut describe: serde_json::Value =
            serde_json::from_str(rendered).expect("valid rendered json");
        let tool = add_view_to_describe(&mut describe, spec).expect("patch describe");
        (describe, tool)
    }

    #[test]
    fn inserts_view_and_ui_permissions() {
        let (v, tool) = patch(RENDERED, &spec());
        assert_eq!(v["contributions"]["views"][0]["id"], "hello");
        assert_eq!(v["contributions"]["views"][0]["entry"], "index.html");
        assert_eq!(v["contributions"]["views"][0]["tools"][0], "echo");
        assert_eq!(
            v["contributions"]["views"][0]["placement"]["slot"],
            "designer.sidebar"
        );
        assert_eq!(v["contributions"]["views"][0]["surface"], "designer");
        assert_eq!(
            v["contributions"]["views"][0]["title_key"],
            "view.hello.label"
        );
        assert!(v["runtime"]["permissions"]["ui"].is_object());
        let tool = tool.expect("a tool was contributed");
        assert_eq!(tool.name, "echo");
        assert_eq!(tool.args, serde_json::json!({}));
    }

    /// Every view field the author can set must reach the describe — an
    /// accepted flag that lands nowhere is worse than a rejected one.
    #[test]
    fn carries_every_configured_view_field_through() {
        let (v, _) = patch(
            RENDERED,
            &ViewSpec {
                id: "usage".to_string(),
                surface: Surface::Admin,
                slot: "admin.tenantDetail".to_string(),
                title_fallback: "Usage".to_string(),
                min_visibility: Visibility::TenantAdmin,
                fetch_hosts: vec!["https://api.acme.com/*".to_string()],
                platform_api: vec![ApiGrant {
                    method: "GET".to_string(),
                    path_pattern: "/api/flows".to_string(),
                }],
            },
        );
        let view = &v["contributions"]["views"][0];
        assert_eq!(view["id"], "usage");
        assert_eq!(view["surface"], "admin");
        assert_eq!(view["title_key"], "view.usage.label");
        assert_eq!(view["title_fallback"], "Usage");
        assert_eq!(view["placement"]["slot"], "admin.tenantDetail");
        assert_eq!(view["min_visibility"], "tenant_admin");

        let ui = &v["runtime"]["permissions"]["ui"];
        assert_eq!(ui["fetchHosts"][0], "https://api.acme.com/*");
        assert_eq!(ui["platformApi"][0]["method"], "GET");
        assert_eq!(ui["platformApi"][0]["path_pattern"], "/api/flows");
    }

    /// `Visibility::Member` is the contract's default and is skipped on the
    /// wire; writing it explicitly would be a diff against every describe the
    /// designer produces for the same view.
    #[test]
    fn the_default_visibility_is_not_written_out() {
        let (v, _) = patch(RENDERED, &spec());
        assert!(
            v["contributions"]["views"][0]
                .get("min_visibility")
                .is_none(),
            "default visibility should be skipped: {}",
            v["contributions"]["views"][0]
        );
    }

    /// The tool name must be derived, not hardcoded: a kind that contributes
    /// a differently-named tool (e.g. `llm`'s `complete`) must see that name
    /// land in `views[0].tools`, not `echo`, and the placeholder args must be
    /// shaped to that tool's own `input_schema`, not `design`'s.
    #[test]
    fn derives_tool_name_and_args_from_whatever_the_kind_actually_contributes() {
        let rendered = r#"{
          "contributions": { "tools": [{
            "name": "complete",
            "input_schema": "{\"type\":\"object\",\"required\":[\"prompt\"],\"properties\":{\"prompt\":{\"type\":\"string\"}}}"
          }] },
          "runtime": { "permissions": {} }
        }"#;
        let (v, tool) = patch(rendered, &spec());
        assert_eq!(v["contributions"]["views"][0]["tools"][0], "complete");
        let tool = tool.expect("a tool was contributed");
        assert_eq!(tool.name, "complete");
        assert_eq!(tool.args, serde_json::json!({ "prompt": "hello" }));
        assert!(
            tool.args.get("message").is_none(),
            "must not carry over echo's `message` shape: {}",
            tool.args
        );
    }

    /// A kind with no `contributions.tools` at all (`bundle`/`deploy`/
    /// `provider`) must scaffold `tools: []`, not a dangling reference. The
    /// contract skips an empty `tools`, so the key is absent rather than `[]`.
    #[test]
    fn empty_tools_when_kind_contributes_none() {
        let rendered = r#"{
          "contributions": {},
          "runtime": { "permissions": {} }
        }"#;
        let (v, tool) = patch(rendered, &spec());
        assert!(
            v["contributions"]["views"][0].get("tools").is_none(),
            "an empty tools list is skipped on the wire: {}",
            v["contributions"]["views"][0]
        );
        assert!(tool.is_none());
    }

    #[test]
    fn rejects_describe_without_contributions() {
        let mut describe: serde_json::Value =
            serde_json::from_str(r#"{"runtime":{"permissions":{}}}"#).unwrap();
        let err = add_view_to_describe(&mut describe, &spec()).unwrap_err();
        assert!(err.to_string().contains("contributions"));
    }

    #[test]
    fn rejects_describe_without_runtime_permissions() {
        let mut describe: serde_json::Value =
            serde_json::from_str(r#"{"contributions":{}}"#).unwrap();
        let err = add_view_to_describe(&mut describe, &spec()).unwrap_err();
        assert!(err.to_string().contains("runtime.permissions"));
    }

    // --- derive_placeholder_args ---

    #[test]
    fn placeholder_args_for_a_string_required_field() {
        let args = derive_placeholder_args(
            r#"{"type":"object","required":["prompt"],"properties":{"prompt":{"type":"string"}}}"#,
        );
        assert_eq!(args, serde_json::json!({ "prompt": "hello" }));
    }

    #[test]
    fn placeholder_args_for_a_mixed_type_schema() {
        let args = derive_placeholder_args(
            r#"{
              "type": "object",
              "required": ["name", "count", "active", "tags", "meta", "unknown_type"],
              "properties": {
                "name": { "type": "string" },
                "count": { "type": "integer" },
                "active": { "type": "boolean" },
                "tags": { "type": "array" },
                "meta": { "type": "object" },
                "unknown_type": { "type": "frobnicator" }
              }
            }"#,
        );
        assert_eq!(
            args,
            serde_json::json!({
                "name": "hello",
                "count": 1,
                "active": true,
                "tags": [],
                "meta": {},
                "unknown_type": "hello"
            })
        );
    }

    #[test]
    fn placeholder_args_missing_input_schema_fields_default_to_empty_object() {
        // No `required` at all.
        assert_eq!(
            derive_placeholder_args(r#"{"type":"object","properties":{}}"#),
            serde_json::json!({})
        );
        // No `required`, no `properties`, nothing.
        assert_eq!(derive_placeholder_args("{}"), serde_json::json!({}));
    }

    #[test]
    fn placeholder_args_unparseable_schema_falls_back_to_empty_object_without_panicking() {
        assert_eq!(
            derive_placeholder_args("not json at all {{{"),
            serde_json::json!({})
        );
    }
}
