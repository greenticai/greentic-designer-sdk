# Authoring secrets in a `describe.json`

Two separate fields govern secrets in a `describe.json`. They serve distinct
purposes and must not be confused.

## 1. `requiredSecrets` — credential declarations

Use the top-level `requiredSecrets` array to declare every credential secret
that an operator must supply before the extension can run.

```json
{
  "requiredSecrets": [
    {
      "key": "tavily/api_key",
      "required": true,
      "description": "Tavily Search API key (from tavily.com)",
      "format": "opaque"
    },
    {
      "key": "slack/bot_token",
      "required": true,
      "description": "Slack bot OAuth token"
    }
  ]
}
```

Each entry is a `SecretRequirement`:

| Field         | Type    | Required | Notes                                               |
|---------------|---------|----------|-----------------------------------------------------|
| `key`         | string  | yes      | Canonical path, e.g. `tavily/api_key`. No leading `/`. |
| `required`    | bool    | yes      | `true` → operator must set it before starting.      |
| `description` | string  | no       | Human-readable hint shown in the installer UI.      |
| `format`      | string  | no       | `"opaque"`, `"url"`, etc. (optional hint).          |

The host resolves a key like `tavily/api_key` from the URI
`secret://tavily/api_key` in the secret store.

## 2. `runtime.permissions.secrets` — read-permission grants

Use `runtime.permissions.secrets` to grant the extension **read access** to
secret namespaces at runtime. This is a list of URI prefixes or wildcards,
not field-name keys.

```json
{
  "runtime": {
    "permissions": {
      "secrets": ["secret://tavily/", "secret://slack/"]
    }
  }
}
```

Valid grant forms:

| Form                  | Meaning                                              |
|-----------------------|------------------------------------------------------|
| `"secret://tavily/"`  | Read any secret under the `tavily/` namespace.       |
| `"*"`                 | Read all secrets (use sparingly).                    |
| `"tavily/"`           | Namespace prefix ending with `/` (alternative form). |

## What NOT to put in `permissions.secrets`

Do not place plain field-name keys (e.g. `"SLACK_BOT_TOKEN"`, `"api_key"`) in
`permissions.secrets`. Such entries do not grant access — they are silently
ignored by the runtime and will fail the `E_PERMS_SECRETS_PLAIN_KEY` lint
check from `gtdx lint`.

Always use `requiredSecrets` for credential field declarations.

## The boundary against `configSchema`

There is a third top-level field that also produces a form in front of an
operator: `configSchema`, a stringly-encoded JSON Schema describing the
extension's **non-secret** configuration — a service URL, a collection name,
a page size. See [authoring-config.md](authoring-config.md).

The two are not interchangeable, and the difference is where the value ends
up. A `configSchema` answer is stored by the admin console and handed to the
guest as the plain tenant overlay: readable back, editable in the form,
present in the stored document. A `requiredSecrets` value goes to the secret
store and reaches the extension through `secret://`, and the console never
holds it.

So: if it is a credential, it goes in `requiredSecrets`, even though
`configSchema` is the field that renders the nicer form. `gtdx lint` reports
a credential-looking property in `configSchema` as `E_CONFIG_SCHEMA_SECRET`
and names `requiredSecrets` in the message.

The reverse is not a rule: non-secret configuration does *not* have to move
to `configSchema`. An extension that has never needed operator configuration
should keep omitting the field rather than declare an empty schema, which
renders as an empty form.

## Summary

```
requiredSecrets        → WHAT secrets the operator must provide (field names)
permissions.secrets    → WHICH namespaces the extension may read at runtime (URI grants)
configSchema           → NON-SECRET operator configuration, rendered as a form
```
