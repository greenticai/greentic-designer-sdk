# Designer Node Types

Design extensions can contribute Designer node type metadata through `describe.json` under `contributions.nodeTypes`. This contract describes the design-time shape a host can read without executing the extension.

The descriptor still keeps `contributions` open for future contribution kinds. Use the typed helpers when you specifically need node type validation:

```rust
let describe: greentic_extension_sdk_contract::DescribeJson = serde_json::from_value(value)?;
let contributions = describe.typed_contributions()?;
```

## Shape

```json
{
  "nodeTypes": [
    {
      "type_id": "example.echo",
      "label": "Echo message",
      "category": "tools",
      "icon": "puzzle",
      "color": "#0d9488",
      "complexity": "simple",
      "config_schema": "{\"type\":\"object\",\"properties\":{\"message\":{\"type\":\"string\"}}}",
      "output_ports": [
        { "name": "success", "label": "Success" },
        { "name": "error", "label": "Error" }
      ]
    }
  ]
}
```

`config_schema` is a JSON Schema encoded as a JSON string. This matches the existing `wasm-component` scaffold and lets the descriptor stay compatible with current extension packages.

## Validation

`greentic-extension-sdk-contract` validates both the JSON shape and semantic rules:

- `type_id`, `label`, and `category` must be non-empty.
- `type_id` values must be unique within `nodeTypes`.
- `config_schema`, when present, must parse as JSON and compile as a JSON Schema.
- output port names must be non-empty and unique per node type.
- known fields and config schema values must not include inline secret-looking values.

The schema asset lives at:

```text
crates/greentic-extension-sdk-contract/schemas/designer-node-types.schema.json
```

## Scaffold

To create a node-providing design extension scaffold:

```bash
gtdx new my-node-extension --kind wasm-component --node-type-id my-node --label "My Node"
```
