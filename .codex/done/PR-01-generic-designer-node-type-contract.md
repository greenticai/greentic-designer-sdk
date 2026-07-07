# PR 01 — Add generic Designer Node Type contribution contract

## Repository

`greenticai/greentic-designer-sdk`

## Objective

Add a typed, generic Designer Node Type contribution contract.

The SDK currently accepts `contributions` as generic JSON, and already has an invariant that a `DesignExtension` with `runtime.gtpack` must contribute non-empty `contributions.nodeTypes`. The existing `wasm-component` scaffold already emits node type entries with fields such as `type_id`, `config_schema`, `output_ports`, `color`, and `complexity`. This PR should formalize that current wire shape, or explicitly include a migration plan if the shape is intentionally changing.

This must remain generic. Do **not** add Sorla, SORX, tenant, flat, claim, order, or any domain-specific types to the SDK.

## Add module

In `greentic-extension-sdk-contract`, add:

```text
src/contributions/mod.rs
src/contributions/node_type.rs
```

Re-export from `lib.rs`.

## Proposed types

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Contributions {
    #[serde(rename = "nodeTypes", default, skip_serializing_if = "Vec::is_empty")]
    pub node_types: Vec<DesignerNodeType>,

    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}
```

Do not combine `#[serde(deny_unknown_fields)]` with the flattened `other` map here. `contributions` needs to remain open for unrelated contribution kinds.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignerNodeType {
    #[serde(rename = "type_id")]
    pub type_id: String,
    pub label: String,
    pub category: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complexity: Option<String>,

    #[serde(rename = "config_schema", default, skip_serializing_if = "Option::is_none")]
    pub config_schema: Option<String>,

    #[serde(rename = "output_ports", default, skip_serializing_if = "Vec::is_empty")]
    pub output_ports: Vec<NodeOutputPort>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NodeOutputPort {
    pub name: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
```

The existing scaffold stores `config_schema` as a JSON string, not an embedded JSON object. Keep that wire shape for compatibility unless this PR also updates the scaffold, tests, docs, and any consumers to a versioned replacement.

## Future component binding

Do not add a required `binding.kind=component`, `operation`, `configSchema`, `inputSchema`, or `outputSchema` shape in this PR unless this is deliberately a breaking or versioned contract migration. Those fields are not emitted by the current `wasm-component` template or tests.

If a new component-binding shape is needed, add it as an optional extension field or a clearly versioned successor to the existing `type_id` model.

## Validation rules

Add validation helpers:

```rust
pub fn validate_contributions(value: &serde_json::Value) -> Result<Contributions, ContractError>;
pub fn validate_node_type(node: &DesignerNodeType) -> Result<(), ContractError>;
```

Rules:

1. `type_id` is required and stable.
2. `label` and `category` are required and non-empty.
3. `config_schema`, when present, must parse as JSON and compile as a JSON Schema.
4. Output port names must be non-empty and unique per node.
5. Node type IDs must be unique.
6. Node type must not include inline secret-looking values in known fields or parsed config schema defaults/examples.

Do not require node-level semver unless a `version` field is added to the existing wire shape with backwards compatibility. Extension versioning already exists at `metadata.version`.

If validation errors need their own variant, add something like:

```rust
#[error("node type contribution is invalid: {0}")]
NodeTypeInvalid(String),
```

to `ContractError`; otherwise use the existing `SchemaInvalid` only when the error truly comes from JSON Schema validation.

## Backwards compatibility

Keep raw `contributions: serde_json::Value` in `DescribeJson` for compatibility, but add typed helpers:

```rust
impl DescribeJson {
    pub fn typed_contributions(&self) -> Result<Contributions, ContractError>;
}
```

Do not break existing extensions.

## Tests

Add tests for:

- valid node type contribution
- duplicate `type_id` values rejected
- invalid `config_schema` JSON rejected
- invalid `config_schema` JSON Schema rejected
- duplicate output port names rejected
- old raw contributions still deserialize
- DesignExtension with `runtime.gtpack` still requires non-empty nodeTypes

## Docs

Add:

```text
docs/node-types.md
```

Update:

- `README.md`
- extension authoring docs if present

## Acceptance criteria

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
bash ci/local_check.sh
```
