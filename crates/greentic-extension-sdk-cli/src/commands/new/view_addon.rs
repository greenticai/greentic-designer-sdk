//! Post-render describe authoring for `gtdx new --with-view`.
//!
//! The view is patched into the rendered `describe.json` rather than shipped
//! as a template overlay because `overlay()` replaces whole files: a
//! `view-addon/describe.json.tmpl` would have to duplicate every kind's
//! describe template and would drift from all of them. `commands::openapi`
//! already authors a describe this way.

/// Insert the example view and its `permissions.ui` block into a rendered
/// describe. Returns the re-serialized document.
pub(super) fn add_view_to_describe(describe_json: &str, view_id: &str) -> anyhow::Result<String> {
    let mut describe: serde_json::Value = serde_json::from_str(describe_json)
        .map_err(|e| anyhow::anyhow!("parse rendered describe.json: {e}"))?;

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
            "tools": ["echo"]
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

    Ok(serde_json::to_string_pretty(&describe)? + "\n")
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
        let out = add_view_to_describe(RENDERED, "hello").expect("patch describe");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(v["contributions"]["views"][0]["id"], "hello");
        assert_eq!(v["contributions"]["views"][0]["entry"], "index.html");
        assert_eq!(v["contributions"]["views"][0]["tools"][0], "echo");
        assert_eq!(
            v["contributions"]["views"][0]["placement"]["slot"],
            "designer.sidebar"
        );
        assert!(v["runtime"]["permissions"]["ui"].is_object());
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
