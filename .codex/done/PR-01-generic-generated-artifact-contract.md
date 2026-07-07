# PR 01 — Add generic generated artifact contract to Designer SDK

## Repository

`greenticai/greentic-designer-sdk`

## Objective

Add a generic generated artifact contract to the SDK so any DesignExtension tool can return design-time outputs through the existing `tools.invoke-tool` JSON string result, such as:

- `.gtpack`
- `.gtbundle`
- OpenAPI overlays
- Arazzo workflows
- MCP tool descriptors
- llms.txt fragments
- deployment plans
- reports

This must remain generic. Do not add Sorla-specific concepts to the SDK.

## Current situation

The SDK already has `runtime.gtpack` metadata for extension runtime packaging, but that is not the same as a tool-generated artifact. Today `runtime.gtpack` is required for `ProviderExtension`, and is also allowed for a `DesignExtension` only when it contributes non-empty `contributions.nodeTypes`. The new contract should describe artifacts produced by extension tools at design time and must not relax or replace those descriptor invariants.

## New contract module

Add to `greentic-extension-sdk-contract`:

```text
src/artifact.rs
```

Suggested types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GeneratedArtifact {
    pub kind: ArtifactKind,
    pub filename: String,
    pub media_type: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct ArtifactKind(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDiagnostic {
    pub severity: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactToolOutput {
    #[serde(default)]
    pub artifacts: Vec<GeneratedArtifact>,
    #[serde(default)]
    pub diagnostics: Vec<ArtifactDiagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_json: Option<serde_json::Value>,
}
```

Use the same JSON field style as the Rust structs unless the implementation explicitly adds serde renames. In this codebase, `runtime.gtpack` currently serializes `pack_id` / `component_version`, so this proposal uses `media_type`, `bytes_base64`, `metadata_json`, and `preview_json` in JSON examples.

## Validation rules

Add helper validation:

```rust
pub fn validate_generated_artifact(artifact: &GeneratedArtifact) -> Result<(), ContractError>;
```

Rules:

1. `filename` must not be empty.
2. `filename` must be relative and must not contain path traversal.
3. `sha256` must be lowercase 64-character hex.
4. At least one of `bytes_base64` or `uri` must be present.
5. `media_type` must not be empty.
6. `kind` must not be empty.
7. If `bytes_base64` is present, decoded bytes must match `sha256`.
8. `uri`, when present, must not be an absolute local filesystem path unless explicitly allowed for local dev fixtures.
9. If both `bytes_base64` and `uri` are present, `bytes_base64` is authoritative for hash validation.

## Suggested media types

Document examples only, not enforced:

```text
application/vnd.greentic.gtpack
application/vnd.greentic.gtbundle
application/vnd.greentic.openapi-overlay+yaml
application/vnd.greentic.arazzo+yaml
application/vnd.greentic.mcp-tools+json
text/plain
application/json
```

## SDK test helpers

In `greentic-extension-sdk-testing`, add helpers:

```rust
pub fn assert_valid_artifact_output_json(json: &str);
pub fn fixture_generated_artifact(kind: &str, filename: &str, bytes: &[u8]) -> GeneratedArtifact;
```

## Documentation

Add:

```text
docs/generated-artifacts.md
```

Explain:

- generated artifact output convention
- how DesignExtension tools can return artifacts via JSON
- how hosts can preview/download/persist them
- difference between descriptor-level `runtime.gtpack` and tool-generated artifacts
- the current `DesignExtension` invariant: `runtime.gtpack` requires non-empty `contributions.nodeTypes`

## Tests

Add tests for:

- valid artifact
- invalid sha
- bytes hash mismatch
- path traversal filename rejected
- empty media type rejected
- JSON roundtrip
- artifact tool output roundtrip

## Acceptance criteria

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
bash ci/local_check.sh
```
