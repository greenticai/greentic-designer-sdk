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

## Summary

```
requiredSecrets        → WHAT secrets the operator must provide (field names)
permissions.secrets    → WHICH namespaces the extension may read at runtime (URI grants)
```
