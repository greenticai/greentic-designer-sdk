# PR 02 — Add node type schema validation and fixtures

## Repository

`greenticai/greentic-designer-sdk`

## Objective

Add canonical JSON schema, fixtures, and tests for `contributions.nodeTypes`.

## New schema

Add a JSON schema asset:

```text
crates/greentic-extension-sdk-contract/schemas/designer-node-types.schema.json
```

The current schema layout is under `crates/greentic-extension-sdk-contract/schemas/`, and `schema.rs` embeds schema assets with `include_str!`. Keep the node type schema there and expose validation through the contract crate.

The schema should validate:

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

This mirrors the current `wasm-component` scaffold. Do not switch the schema to `id`/`version`/`binding`/`configSchema` unless PR 01 explicitly introduces a versioned migration away from the existing shape.

## Fixtures

Add fixtures:

```text
crates/greentic-extension-sdk-testing/fixtures/node-types/valid-wasm-component-node.json
crates/greentic-extension-sdk-testing/fixtures/node-types/invalid-bad-config-schema.json
crates/greentic-extension-sdk-testing/fixtures/node-types/invalid-duplicate-node-type-id.json
crates/greentic-extension-sdk-testing/fixtures/node-types/valid-business-action-node.json
```

There is no current root-level `fixtures/` directory. If static JSON fixtures are added, either embed them from the testing crate or place test-only fixtures beside the tests that consume them.

The business-action fixture must remain generic and must not require SDK Sorla/SORX types. It can show arbitrary metadata in config schema, for example:

```json
{
  "action_ref": {
    "const": {
      "id": "record_business_event",
      "version": "0.1.0",
      "contract_hash": "sha256:..."
    }
  }
}
```

## Testing helpers

In `greentic-extension-sdk-testing`, add:

```rust
pub fn load_node_type_fixture(name: &str) -> serde_json::Value;
pub fn assert_valid_node_type_contributions(value: &serde_json::Value);
pub fn assert_invalid_node_type_contributions(value: &serde_json::Value);
```

Wire these through `crates/greentic-extension-sdk-testing/src/lib.rs`, matching the existing `artifact` helper pattern.

## Tests

Add tests for:

- all valid fixtures pass
- invalid fixtures fail with useful error
- node type schema validates contributions
- typed Rust model roundtrips to JSON
- `DescribeJson::typed_contributions()` works
- the existing `wasm-component` scaffold's `contributions.nodeTypes` passes validation

## Docs

Update:

```text
docs/node-types.md
```

Add an example node type using the current scaffold-backed shape. Only call it “component-backed” if the contract has an explicit binding field by the time this PR lands.

## Acceptance criteria

```bash
cargo test --workspace --all-features
bash ci/local_check.sh
```
