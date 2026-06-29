use greentic_extension_sdk_contract::describe::{Knowledge, Prompt, Recipe, Schema};

#[test]
fn recipe_parses() {
    let v = serde_json::json!({
        "id": "standard",
        "display_name": "Standard Greentic Pack",
        "description": "Package designer session into a .gtpack archive",
        "config_schema": "schemas/standard.config.schema.json"
    });
    let r: Recipe = serde_json::from_value(v).unwrap();
    assert_eq!(r.id, "standard");
    assert_eq!(r.display_name.default(), "Standard Greentic Pack");
}

#[test]
fn knowledge_accepts_directory_string() {
    let v = serde_json::json!({ "path": "knowledge/" });
    let k: Knowledge = serde_json::from_value(v).unwrap();
    assert_eq!(k.path, "knowledge/");
}

#[test]
fn prompt_accepts_file_string() {
    let v = serde_json::json!({ "path": "prompts/rules.md" });
    let p: Prompt = serde_json::from_value(v).unwrap();
    assert_eq!(p.path, "prompts/rules.md");
}

#[test]
fn schema_accepts_file_string() {
    let v = serde_json::json!({ "path": "schemas/adaptive-card-v1.6.json" });
    let s: Schema = serde_json::from_value(v).unwrap();
    assert_eq!(s.path, "schemas/adaptive-card-v1.6.json");
}
