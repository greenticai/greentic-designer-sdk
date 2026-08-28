# Authoring `configSchema` — extension-level operator configuration

`configSchema` is a top-level field in `describe.json` holding a JSON Schema
(Draft 2020-12) for the **non-secret** configuration an operator sets for
this extension in a tenant: a service URL, a collection name, a page size.

```json
{
  "apiVersion": "greentic.ai/v2",
  "configSchema": "{\"type\":\"object\",\"properties\":{\"service_url\":{\"type\":\"string\",\"format\":\"uri\",\"title\":\"Service URL\",\"description\":\"Base URL of the Qdrant instance.\"},\"collection\":{\"type\":\"string\",\"title\":\"Collection\",\"default\":\"documents\"}},\"required\":[\"service_url\"]}"
}
```

The admin console renders that schema as a form, stores the operator's
answers, and delivers them to the guest as the `_tenant_overlay`. Without the
field the console has nothing to render from and falls back to a raw JSON
textarea, which a non-technical operator cannot use. That fallback is the
whole reason the field exists.

## Why top-level, and not per contribution

Three other fields in the contract are also called `config_schema`, on
`Recipe`, `Addon` and `NodeType`. Those are per-contribution because each
configures one thing: one palette entry, one provisioned service, one recipe.

This one is per-extension, because the document it describes is
per-extension. There is one tenant overlay per installed extension, shared by
every view, tool and component it ships — so there is one form, and one
schema, at the top level. It sits beside `requiredSecrets`, the other half of
the same operator-setup story, which is top-level for the same reason.

**The exact JSON path is `configSchema`, at the root of `describe.json`** —
camelCase, matching its top-level neighbours (`apiVersion`,
`manifestSha256`, `requiredSecrets`) rather than the snake_case used inside
`contributions`.

## Rules

- **Optional.** Omit it if the extension has no operator configuration. Do
  not declare `{"type":"object"}` with no properties to "be explicit": that
  renders as an empty form, which reads as a broken page rather than as
  "nothing to configure here".
- **A string, not an inline object.** Stringly-encoded for the same reason
  `NodeType.config_schema` is: it is a payload passed through to a renderer,
  not host control data.
- **Must parse, and must parse to a JSON object.** `"42"` and `"null"` are
  valid JSON but render as an empty form with *no error at all* — the worst
  place to discover a typo. Both the contract deserializer and `gtdx lint`
  (`E_CONFIG_SCHEMA_INVALID`) reject them.
- **No credentials.** See below.

## Secrets do not go here

A credential goes in the top-level `requiredSecrets`, never in
`configSchema`. The difference is where the value is stored: a `configSchema`
answer lives in the tenant overlay in the clear and is read back into the
form, while a `requiredSecrets` value goes to the secret store and reaches
the extension through `secret://`.

`gtdx lint` reports a credential-looking property in `configSchema` as
`E_CONFIG_SCHEMA_SECRET`. The detector is the same one the addon rule uses,
including its carve-outs: `secret_ref` and `password_policy` are a reference
and a policy, not credentials, and are not flagged.

See [authoring-secrets.md](authoring-secrets.md) for the full boundary.

## Compatibility

`DescribeJson` is `#[serde(deny_unknown_fields)]` and `describe-v2.json`'s
root is `additionalProperties: false`, so a host built against a contract
crate older than this field **rejects** a describe carrying it, at both
layers, naming the key. It does not half-parse it and it does not silently
drop it.

That is the wanted direction — an extension whose configuration would be
discarded should fail to load rather than run misconfigured — but it does
mean a `configSchema`-bearing describe requires a host that knows the field.
`compat::MIN_DESIGNER_VERSION` is deliberately *not* raised for this, for the
same reason it was not raised for `views` or `addons`: it is stamped into
every describe `gtdx new` scaffolds, so raising it would gate installs of
extensions that carry none of these fields. See the note on that constant.

## Not scaffolded

`gtdx new` does not emit a `configSchema`, including with `--with-view`.
Unlike the three per-contribution `config_schema` fields — which are
*required* on the contributions that have them, so a scaffold must emit
something — this one is optional, and most extensions genuinely have no
operator configuration. A scaffolded placeholder would either be an empty
object (an empty form in the console, worse than the field's absence) or a
plausible-looking example the author must remember to delete. Add it by hand
when the extension actually has a knob.
