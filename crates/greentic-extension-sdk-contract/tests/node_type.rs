use greentic_extension_sdk_contract::ComponentId;
use greentic_extension_sdk_contract::describe::{NodeType, OutputPort};
use std::str::FromStr;

#[test]
fn parses_minimal_node_type() {
    let v = serde_json::json!({
        "type_id": "adaptive-card",
        "label": "Adaptive Card",
        "category": "visual",
        "icon": "🎴",
        "color": "#0d9488",
        "complexity": "complex",
        "config_schema": "{}",
        "output_ports": [{ "name": "default", "label": "Next" }]
    });
    let nt: NodeType = serde_json::from_value(v).unwrap();
    assert_eq!(nt.type_id, "adaptive-card");
    assert_eq!(nt.label.default(), "Adaptive Card");
    assert!(nt.runtime_ref.is_none());
    assert!(nt.deprecated.is_none());
    assert_eq!(nt.output_ports.len(), 1);
}

#[test]
fn parses_node_type_with_runtime_ref() {
    let v = serde_json::json!({
        "type_id": "adaptive-card",
        "label": "Adaptive Card",
        "category": "visual",
        "icon": "🎴",
        "color": "#0d9488",
        "complexity": "complex",
        "config_schema": "{}",
        "output_ports": [],
        "runtime_ref": "adaptive-card"
    });
    let nt: NodeType = serde_json::from_value(v).unwrap();
    assert_eq!(
        nt.runtime_ref.unwrap(),
        ComponentId::from_str("adaptive-card").unwrap()
    );
}

#[test]
fn parses_node_type_with_localized_label() {
    let v = serde_json::json!({
        "type_id": "adaptive-card",
        "label": { "default": "Adaptive Card", "locales": { "id": "Kartu Adaptif" } },
        "category": "visual",
        "icon": "🎴",
        "color": "#0d9488",
        "complexity": "complex",
        "config_schema": "{}",
        "output_ports": []
    });
    let nt: NodeType = serde_json::from_value(v).unwrap();
    use greentic_extension_sdk_contract::Locale;
    let id = Locale::from_str("id").unwrap();
    assert_eq!(nt.label.lookup(&id), Some("Kartu Adaptif"));
}

#[test]
fn parses_deprecated_node_type() {
    let v = serde_json::json!({
        "type_id": "old-node",
        "label": "Old",
        "category": "visual",
        "icon": "x",
        "color": "#000",
        "complexity": "simple",
        "config_schema": "{}",
        "output_ports": [],
        "deprecated": { "since": "1.4.0", "removal_in": "2.0.0" }
    });
    let nt: NodeType = serde_json::from_value(v).unwrap();
    let dep = nt.deprecated.unwrap();
    assert_eq!(dep.since.to_string(), "1.4.0");
}

#[test]
fn unknown_fields_rejected() {
    let v = serde_json::json!({
        "type_id": "x",
        "label": "x",
        "category": "x",
        "icon": "x",
        "color": "#000",
        "complexity": "simple",
        "config_schema": "{}",
        "output_ports": [],
        "wat": "wat"
    });
    let r: Result<NodeType, _> = serde_json::from_value(v);
    assert!(r.is_err());
}

#[test]
fn output_port_label_localized() {
    let v = serde_json::json!({ "name": "yes", "label": { "default": "Match", "locales": { "id": "Cocok" } } });
    let p: OutputPort = serde_json::from_value(v).unwrap();
    assert_eq!(p.name, "yes");
    assert_eq!(p.label.default(), "Match");
}
