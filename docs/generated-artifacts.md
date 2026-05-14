# Generated Artifacts

Design extension tools can return generated artifacts as JSON through the existing `tools.invoke-tool` WIT function. This convention is for design-time outputs such as `.gtpack` files, bundle recipes, overlays, workflows, tool descriptors, reports, or plain text fragments.

This is separate from descriptor-level `runtime.gtpack`. `runtime.gtpack` describes runtime packaging in `describe.json`: it is required for `ProviderExtension`, and a `DesignExtension` may include it only when it contributes non-empty `contributions.nodeTypes`.

## Output Shape

Tools that produce artifacts should return an `ArtifactToolOutput` JSON object:

```json
{
  "artifacts": [
    {
      "kind": "example",
      "filename": "example-artifact.json",
      "media_type": "application/json",
      "sha256": "64 lowercase hex characters",
      "bytes_base64": "base64 payload"
    }
  ],
  "diagnostics": [],
  "preview_json": {
    "title": "Example artifact"
  }
}
```

An artifact must include a non-empty kind, safe relative filename, media type, and lowercase SHA-256 digest. It must include either inline `bytes_base64` or a `uri`. If inline bytes are present, hosts can verify the bytes against `sha256` before previewing, downloading, or persisting the file.

## Suggested Media Types

These are examples only; validation does not restrict the media type list.

```text
application/vnd.greentic.gtpack
application/vnd.greentic.gtbundle
application/vnd.greentic.openapi-overlay+yaml
application/vnd.greentic.arazzo+yaml
application/vnd.greentic.mcp-tools+json
text/plain
application/json
```

Hosts may use `preview_json` to render a summary while still treating the artifact bytes or URI as the authoritative output.

## Artifact-Producing Design Extension

Use the artifact producer scaffold when you want a concrete DesignExtension example:

```bash
gtdx new artifact-demo --kind design-artifact-producer
```

The scaffold exposes a `generate_artifact` tool through the existing design-extension `tools` interface. It advertises `schemas/generate-artifact.input.schema.json` and `schemas/artifact-output.schema.json`, and includes `examples/artifact-output.json` as a static sample of the generated artifact output shape.
