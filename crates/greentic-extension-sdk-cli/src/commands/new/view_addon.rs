//! Post-render describe authoring for `gtdx new --with-view`.
//!
//! The view is patched into the rendered `describe.json` rather than shipped
//! as a template overlay because `overlay()` replaces whole files: a
//! `view-addon/describe.json.tmpl` would have to duplicate every kind's
//! describe template and would drift from all of them. `commands::openapi`
//! already authors a describe this way.

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

/// Insert the example view and its `permissions.ui` block into a rendered
/// describe. Returns the re-serialized document and the chosen tool (if
/// any), so the caller can render the example page's `{{view_tool}}` and
/// `{{view_tool_args}}` placeholders to match.
pub(super) fn add_view_to_describe(
    describe_json: &str,
    view_id: &str,
) -> anyhow::Result<(String, Option<ChosenTool>)> {
    let mut describe: serde_json::Value = serde_json::from_str(describe_json)
        .map_err(|e| anyhow::anyhow!("parse rendered describe.json: {e}"))?;

    let chosen_tool = first_contributed_tool(&describe);
    let tools_value = match &chosen_tool {
        Some(tool) => serde_json::json!([tool.name]),
        None => serde_json::json!([]),
    };

    let contributions = describe
        .get_mut("contributions")
        .and_then(|c| c.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("rendered describe.json has no contributions object"))?;

    contributions.insert(
        "views".to_string(),
        serde_json::json!([{
            "id": view_id,
            "surface": "designer",
            "title_key": format!("view.{view_id}.label"),
            "title_fallback": "Hello",
            "entry": "index.html",
            "placement": { "slot": "designer.sidebar" },
            "tools": tools_value
        }]),
    );

    let permissions = describe
        .pointer_mut("/runtime/permissions")
        .and_then(|p| p.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("rendered describe.json has no runtime.permissions"))?;
    permissions.insert(
        "ui".to_string(),
        serde_json::json!({ "fetchHosts": [], "platformApi": [] }),
    );

    Ok((serde_json::to_string_pretty(&describe)? + "\n", chosen_tool))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RENDERED: &str = r#"{
  "contributions": { "tools": [{ "name": "echo" }] },
  "runtime": { "permissions": { "network": [] } }
}"#;

    #[test]
    fn inserts_view_and_ui_permissions() {
        let (out, tool) = add_view_to_describe(RENDERED, "hello").expect("patch describe");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(v["contributions"]["views"][0]["id"], "hello");
        assert_eq!(v["contributions"]["views"][0]["entry"], "index.html");
        assert_eq!(v["contributions"]["views"][0]["tools"][0], "echo");
        assert_eq!(
            v["contributions"]["views"][0]["placement"]["slot"],
            "designer.sidebar"
        );
        assert!(v["runtime"]["permissions"]["ui"].is_object());
        let tool = tool.expect("a tool was contributed");
        assert_eq!(tool.name, "echo");
        assert_eq!(tool.args, serde_json::json!({}));
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
        let (out, tool) = add_view_to_describe(rendered, "hello").expect("patch describe");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
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
    /// `provider`) must scaffold `tools: []`, not a dangling reference.
    #[test]
    fn empty_tools_when_kind_contributes_none() {
        let rendered = r#"{
          "contributions": {},
          "runtime": { "permissions": {} }
        }"#;
        let (out, tool) = add_view_to_describe(rendered, "hello").expect("patch describe");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(
            v["contributions"]["views"][0]["tools"]
                .as_array()
                .expect("tools array"),
            &Vec::<serde_json::Value>::new()
        );
        assert!(tool.is_none());
    }

    #[test]
    fn rejects_describe_without_contributions() {
        let err = add_view_to_describe(r#"{"runtime":{"permissions":{}}}"#, "hello").unwrap_err();
        assert!(err.to_string().contains("contributions"));
    }

    #[test]
    fn rejects_describe_without_runtime_permissions() {
        let err = add_view_to_describe(r#"{"contributions":{}}"#, "hello").unwrap_err();
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
