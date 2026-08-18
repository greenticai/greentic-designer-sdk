# Connector Catalog

Worked OpenAPI specs that the `gtdx openapi` generator turns into ready-to-publish Greentic connectors (`DesignExtension` components). Each spec here fits the generator's v1 scope — OpenAPI 3.0, `GET`/`POST` operations with an `operationId`, `application/json` request bodies, `path`+`query` parameters, and a single `bearer` or header `apiKey` security scheme.

## Catalog

| Connector | Spec | Tools | Auth |
|-----------|------|-------|------|
| **Todoist** (REST v2) | [`todoist.openapi.json`](./todoist.openapi.json) | `getTasks`, `createTask`, `getTask`, `closeTask`, `getProjects`, `createProject` | `bearer` |

## Generate a connector from a spec

```bash
# 1. Generate the DesignExtension connector from the spec
gtdx openapi examples/connectors/todoist.openapi.json --out ./out --name todoist

# 2. Build the WASM component (cargo component build -> wasm32-wasip2)
cd ./out/todoist && ./build.sh

# 3. Publish it (pre-built wasm handed to gtdx publish)
gtdx publish --wasm target/wasm32-wasip2/release/*.wasm .
```

The generator derives the connector's `describe.json`:
- **`kind: DesignExtension`** — exports `greentic:extension-design/tools`, imports the host `http` + `secrets` capabilities.
- **`runtime.permissions.network`** from the spec's `servers[]` (e.g. Todoist → `https://api.todoist.com/rest/v2/*`).
- One **tool per operation**, named by `operationId`, its `input-schema` merging the operation's path/query parameters and the JSON request body.
- The `bearer`/`apiKey` scheme → a `secret://<connector>/<scheme>` reference (the token is injected by the host, never embedded).

## Runtime behavior

Each tool's `invoke` substitutes path parameters into the URL, appends query parameters, attaches the bearer/apiKey credential from the referenced secret, calls the host `http.fetch`, and returns a `{status, ok, body}` envelope.

## Adding a connector

Drop a new `<name>.openapi.json` here (staying within the v1 scope above), add a row to the table, and it is catalog-ready. Deferred spec features (`oneOf`/`allOf`, multipart bodies, OAuth2 flows, cookie/header params, response→output schemas) are out of the generator's v1 scope — operations using only unsupported constructs are skipped with a warning rather than mis-generated.

## Verification

Generation + compilation of the Todoist connector is exercised offline: `crates/greentic-extension-sdk-cli`'s parser tests assert the spec yields its six tools with the correct auth + base URL. Calling the real third-party API requires a live token and is a deployment-time (pre-enablement) step, not part of CI.
