//! Post-render describe authoring for `gtdx new --with-view`.
//!
//! The view is patched into the rendered `describe.json` rather than shipped
//! as a template overlay because `overlay()` replaces whole files: a
//! `view-addon/describe.json.tmpl` would have to duplicate every kind's
//! describe template and would drift from all of them. `commands::openapi`
//! already authors a describe this way.

/// Pick the tool the example view is allowed to call: the first entry in
/// `contributions.tools[]`, whatever that kind happens to contribute.
///
/// The deserializer invariant in `greentic-extension-sdk-contract` rejects any
/// `views[].tools` entry that doesn't name a tool the extension itself
/// contributes, so this must never hardcode a name — kinds differ (`design`
/// contributes `echo`, `llm` contributes `complete`, `bundle`/`deploy`/
/// `provider` contribute none at all). `None` means the kind contributes no
/// tools; the view still ships, it just can't call one yet.
fn first_contributed_tool_name(describe: &serde_json::Value) -> Option<String> {
    describe
        .pointer("/contributions/tools")
        .and_then(|tools| tools.as_array())
        .and_then(|tools| tools.first())
        .and_then(|tool| tool.get("name"))
        .and_then(|name| name.as_str())
        .map(str::to_string)
}

/// Insert the example view and its `permissions.ui` block into a rendered
/// describe. Returns the re-serialized document and the tool name (if any)
/// bound into `views[0].tools`, so the caller can render the example page's
/// `{{view_tool}}` placeholder to match.
pub(super) fn add_view_to_describe(
    describe_json: &str,
    view_id: &str,
) -> anyhow::Result<(String, Option<String>)> {
    let mut describe: serde_json::Value = serde_json::from_str(describe_json)
        .map_err(|e| anyhow::anyhow!("parse rendered describe.json: {e}"))?;

    let tool_name = first_contributed_tool_name(&describe);
    let tools_value = match &tool_name {
        Some(name) => serde_json::json!([name]),
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

    Ok((serde_json::to_string_pretty(&describe)? + "\n", tool_name))
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
        let (out, tool_name) = add_view_to_describe(RENDERED, "hello").expect("patch describe");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(v["contributions"]["views"][0]["id"], "hello");
        assert_eq!(v["contributions"]["views"][0]["entry"], "index.html");
        assert_eq!(v["contributions"]["views"][0]["tools"][0], "echo");
        assert_eq!(
            v["contributions"]["views"][0]["placement"]["slot"],
            "designer.sidebar"
        );
        assert!(v["runtime"]["permissions"]["ui"].is_object());
        assert_eq!(tool_name.as_deref(), Some("echo"));
    }

    /// The tool name must be derived, not hardcoded: a kind that contributes
    /// a differently-named tool (e.g. `llm`'s `complete`) must see that name
    /// land in `views[0].tools`, not `echo`.
    #[test]
    fn derives_tool_name_from_whatever_the_kind_actually_contributes() {
        let rendered = r#"{
          "contributions": { "tools": [{ "name": "complete" }] },
          "runtime": { "permissions": {} }
        }"#;
        let (out, tool_name) = add_view_to_describe(rendered, "hello").expect("patch describe");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(v["contributions"]["views"][0]["tools"][0], "complete");
        assert_eq!(tool_name.as_deref(), Some("complete"));
    }

    /// A kind with no `contributions.tools` at all (`bundle`/`deploy`/
    /// `provider`) must scaffold `tools: []`, not a dangling reference.
    #[test]
    fn empty_tools_when_kind_contributes_none() {
        let rendered = r#"{
          "contributions": {},
          "runtime": { "permissions": {} }
        }"#;
        let (out, tool_name) = add_view_to_describe(rendered, "hello").expect("patch describe");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(
            v["contributions"]["views"][0]["tools"]
                .as_array()
                .expect("tools array"),
            &Vec::<serde_json::Value>::new()
        );
        assert_eq!(tool_name, None);
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
}
