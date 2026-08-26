# Authoring addons

An addon is catalogue metadata for a managed service your extension offers
to an environment — Qdrant, Redis, Postgres. You declare its id, what family
of thing it is, the config form a user fills in per environment, the day-2
state a reconciler will manage, and the values it hands back once running.
That declaration ships inside your `.gtxpack` as `contributions.addons[]`.

> **Phase 1 status — read this before you publish one.** Today's SDK lets
> you declare an addon, have `gtdx validate` enforce its invariants, and have
> `gtdx lint` catch the mistakes a schema can't. **Declaring an addon does
> not deploy anything.** Nothing in the platform reconciles it yet — no
> `observe`, no `plan`, no `apply` runs against your `desired_state_schema`.
> The platform owns provisioning (spec D6): in phase 1 that means platform-side
> reconcilers for a short first-party list (Qdrant, Redis, Postgres), written
> against the same interface a third party will eventually use. Your
> extension can *offer* an addon by shipping this catalogue entry; only a
> platform release that recognizes your `family` and runs a matching
> reconciler will ever provision one.
>
> A third party shipping their *own* reconciler as WASM — the
> `AddonExtension` kind, the `extension-addon` WIT world — is phase 2, and it
> is gated on a contract release (`extension-base@0.3.0`) this repo does not
> ship. Adding that kind is a breaking WIT change: `extension-base.wit`'s
> `kind` enum would need a variant, and because `manifest.get-identity()`
> references it, every existing world would need the runtime to serve two
> `manifest` versions at once. That is cross-repo coordination, not a
> feature branch. `ExtensionKind` stays at five variants until it lands.
>
> Read only the schema and you would reasonably conclude that declaring an
> addon provisions one. It doesn't, yet.
>
> **There is also a compat trap, and it is the more serious half of this
> note.** `Contributions` is `#[serde(deny_unknown_fields)]`. A designer
> built against a pre-addons contract crate does not skip an unrecognised
> `addons` key — it fails to parse your `describe.json` at all, so the
> whole extension refuses to load. Meanwhile `gtdx new` fills
> `compat.min_designer_version` from this SDK's `MIN_DESIGNER_VERSION`
> constant, currently `1.2.0`. So an addon-bearing describe *claims*
> `>=1.2.0` while actually being unloadable on every designer released to
> date — the entire released line as of this writing. Do not publish an
> `addons[]`-carrying extension for general use until a host release that
> understands it has shipped; until then, `contributions.addons[]` is
> something you develop and validate locally, not something you ship.

The worked example below is Qdrant throughout, because Qdrant is the addon
spec §9.3 names as the first one built.

## Declaring it

    "contributions": {
      "addons": [{
        "id": "qdrant",
        "family": "vector-db",
        "display_name": "Qdrant",
        "description": "Vector database for embeddings and similarity search.",
        "icon": "database",
        "config_schema": "{\"type\":\"object\",\"properties\":{\"replicas\":{\"type\":\"integer\",\"minimum\":1,\"default\":1},\"size_gb\":{\"type\":\"integer\",\"minimum\":1,\"default\":10}},\"required\":[\"size_gb\"]}",
        "desired_state_schema": "{\"type\":\"object\",\"properties\":{\"collections\":{\"type\":\"array\",\"items\":{\"type\":\"string\"}}}}",
        "outputs": [
          { "name": "URL", "type": "text", "description": "Base URL of the running instance." },
          { "name": "API_KEY", "type": "text", "sensitive": true, "description": "Admin API key." }
        ],
        "supports_backup": true,
        "schema_version": 1
      }]
    }

## Every field

- **`id`** — unique within the extension. The platform namespaces it as
  `<extension_id>/<id>`, so `qdrant` here becomes globally addressable
  without you having to pick a globally-unique string. Must match
  `^[a-z0-9][a-z0-9-]*$` — lowercase letters, digits and hyphens only, no
  underscore, no dot. That's a narrower pattern than a view's `id`
  (`^[a-z0-9][a-z0-9._-]*$`); `gtdx lint` enforces it as `E_ADDON_ID_PATTERN`.

- **`family`** — what kind of thing this is, e.g. `vector-db`, not which
  vendor. A flow that needs a vector database asks for the family, so a
  deployment can substitute one implementation for another. It's an open
  string, not a closed enum, for the same reason a view's `slot` is open:
  `describe.json` is signed and immutable once published, so a closed enum
  baked into it rots the moment the platform adds a family this SDK didn't
  know about. `gtdx lint` warns rather than errors on an unfamiliar family —
  see `W_ADDON_FAMILY_UNKNOWN` below.

- **`display_name`**, **`description`** — presentation only. These render in
  the Designer's catalogue, the most user-facing surface an addon appears on,
  but they are plain `String` today, unlike `Recipe.display_name` and
  `NodeType.label` (`LocalizedString`) or `View.title_key`/`title_fallback`.
  Addon catalogue strings are not localizable yet; expect that to change.
  `LocalizedString` deserializes a bare string transparently, so the switch
  will be wire-compatible in both directions when it happens.

- **`icon`** — optional, presentation only. A **host-resolved icon name**,
  matching `View.icon` — not a file path. The host looks the name up in its
  own icon set; nothing in the packer copies an addon icon, and no
  directory is reserved for one the way `assets/views/<id>/` is for a view.

- **`config_schema`** — JSON Schema (Draft 2020-12), stringly-encoded, for
  the knobs a user sets per environment: size, replica count, version. The
  Designer renders this as a form. It's a string rather than an inline
  object for the same reason `NodeType.config_schema` is: it's a payload
  handed to a renderer, not host control data. Must parse as JSON — the
  Rust deserializer rejects the describe outright if it doesn't, naming the
  field in the error.

- **`desired_state_schema`** — JSON Schema for the day-2 state a reconciler
  manages: Qdrant collections, Redis ACL users. Same stringly-encoded
  treatment as `config_schema`, same parse-time enforcement. **Secrets do
  not belong here** — see the section below.

- **`outputs`** — values the addon publishes once provisioned, referenced
  from another resource's configuration as
  `${resources.<resource_id>.outputs.<name>}`. Each entry needs a `name`
  and a `type`; `sensitive` and `description` are optional. See "The
  `sensitive` flag" below.

- **`supports_backup`** — whether the addon can snapshot before a
  destructive change. The platform offers to back up on the strength of
  this flag alone, so declare `true` only when a snapshot genuinely
  happens — there's no code path that verifies the claim.

- **`schema_version`** — see its own section below.

There's no `gtdx new --with-addon` scaffold in phase 1 the way there's
`--with-view`; you hand-write `contributions.addons[]` and validate it with
`gtdx validate` and `gtdx lint`.

## Why secrets do not go in `desired_state_schema`

A reconciler's job is to compare `desired_state_schema` against what
`observe` reports back from the running service, and reconcile the
difference. That round trip is the whole mechanism, and it's why a secret
breaks it: `observe` reads the *running* Qdrant, Redis, or Postgres, and
none of those services will ever hand a password back out through their
inspection API. So a credential property in `desired_state_schema` diffs
against nothing, forever — `observe` reports no value, `desired_state_schema`
demands one, and no plan is ever clean. Every reconciliation from then on
sees drift that doesn't exist.

Credentials reach an addon a different way entirely: through its runtime
binding, not through desired state. That path is unaffected by this rule —
this is only about what you put in the schema string in your `describe.json`.

`gtdx lint` catches a property that looks like a credential with
`E_ADDON_SECRET_IN_DESIRED_STATE`. It walks the *entire* shape of
`desired_state_schema` (after parsing it as JSON) — every property name
reachable through `properties`, `items`, `$defs`, `definitions`,
`patternProperties`, `additionalProperties`, and the `allOf`/`anyOf`/`oneOf`
branches — not just the top level. That matters for the shape day-2 state
usually takes: a list of managed objects, like Redis ACL users:

    "desired_state_schema": "{\"type\":\"object\",\"properties\":{\"acl_users\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"username\":{\"type\":\"string\"},\"password\":{\"type\":\"string\"}}}}}}"

is flagged with the path `acl_users[].password`, even though `password` is
nested two levels deep inside `items.properties`.

A property name is checked against a list of markers — `password`, `secret`,
`apikey`, `credential`, `passwd` — matched case-insensitively with `-` and
`_` stripped, so `api_key`, `apiKey`, and `api-key` all trip it. But the raw
marker match is narrowed by the property name's *head noun* (its final
segment, split on `-`, `_`, and camelCase boundaries) and, separately, by a
predicate first segment:

- A benign head noun exempts the whole name, even when an earlier segment
  contains a marker: `password_policy` (`policy`), `min_password_length`
  (`length`), `password_encryption` (`encryption`), `secret_ref` (`ref`),
  `secret_name` (`name`), `secretKeyRef` (`ref`), `api_key_id` (`id`),
  `credential_rotation_days` (`days`), `secrets_backend` (`backend`) — none
  of these hold a credential's value.
- A predicate-prefix first segment exempts the whole name too:
  `require_password`, `allow_credentials` — these ask a yes/no question
  about a credential concept, not the value.

`token` is handled the same head-noun way, on its own: it only matches when
`token` is the final segment. `auth_token` is a token, held in the `auth`
slot, and gets flagged; `max_tokens` (segment "tokens", plural) and
`tokenizer` (no segment boundary at all — the whole word is "tokenizer") are
not, because they aren't credentials, they're a count and a component name
that happen to contain the letters.

`config_schema` is never checked by this rule — config isn't reconciled
against observed state, so it can't diff.

Fix by removing the property from `desired_state_schema` and moving the
credential to wherever your addon's runtime binding delivers it. If day-2
state needs to *reference* where a credential lives — the shape D16
recommends — name the property with a `ref`/`name` head noun rather than the
credential itself:

    // Before — flagged as E_ADDON_SECRET_IN_DESIRED_STATE
    "desired_state_schema": "{\"type\":\"object\",\"properties\":{\"admin_password\":{\"type\":\"string\"}}}"

    // After — either drop it entirely, or reference where it lives
    "desired_state_schema": "{\"type\":\"object\",\"properties\":{\"collections\":{\"type\":\"array\",\"items\":{\"type\":\"string\"}},\"admin_secret_ref\":{\"type\":\"string\"}}}"

## The `sensitive` flag

An output with `"sensitive": true` never becomes a literal value anywhere
downstream. The platform resolves it to a secret reference instead —
`valueFrom.secretKeyRef` on Kubernetes, a `sensitive` variable in generated
IaC — which means the actual value never passes through a plan document, a
plan UI, or a support bundle. Everything that would normally show a
resolved output shows the reference instead.

Getting the flag wrong in either direction has a real cost. Leave it `false`
on a value that's actually secret — an API key, an admin password — and that
value is printed in plans, shown in the UI, and captured verbatim in a
support bundle the first time someone attaches one to a ticket. Set it
`true` on a value that's genuinely public, like a hostname, and you make an
ordinary piece of config harder to read than it needs to be — an
inconvenience, not a leak. So default to `true` for anything you wouldn't
paste into a chat channel, and only mark `false` what you'd be fine
publishing.

In the Qdrant example above, `URL` is not sensitive — a hostname isn't a
secret, and other resources need to see it in a plan to know what they're
binding to. `API_KEY` is.

## `schema_version`

`schema_version` versions this addon's `desired_state_schema` — not the
addon itself, not `config_schema`, not the extension's own version. It
exists so that when you need to change the *shape* of the state your
reconciler manages — say, Qdrant collections gain a `shards` field that
didn't used to exist — you can bump `schema_version` to `2` and migrate
existing instances from the v1 shape to the v2 shape, rather than the change
breaking every environment that already declared a v1 instance. It defaults
to `1`, so an addon declared before this field existed, or one that's never
needed to change shape, stays valid without touching it.

## Lint codes

| Code | Meaning |
|---|---|
| `E_ADDON_ID_PATTERN` | `id` does not match `^[a-z0-9][a-z0-9-]*$` |
| `E_ADDON_OUTPUT_NAME` | an output `name` does not match `^[A-Za-z_][A-Za-z0-9_]*$` — outputs are injected as environment variables on the consuming service, so the name has to survive that |
| `E_ADDON_SECRET_IN_DESIRED_STATE` | a property of `desired_state_schema`, at any depth, looks like a credential (see above) |
| `W_ADDON_FAMILY_UNKNOWN` | `family` is not one this `gtdx` build recognizes |

Fixes, in order:

- **`E_ADDON_ID_PATTERN`** — rename `id` to use only lowercase letters,
  digits, and hyphens, starting with a letter or digit. `qdrant-primary` is
  fine; `Qdrant_Primary` and `qdrant.primary` are not (both are legal for a
  view's `id`, neither is for an addon's).
- **`E_ADDON_OUTPUT_NAME`** — rename the output to a valid identifier:
  letters, digits and underscores, not starting with a digit. Hyphens are
  not allowed here even though they are in `id`, because the name becomes
  an environment variable verbatim — `api-key` isn't a legal shell variable
  name, `api_key` or `API_KEY` is.
- **`E_ADDON_SECRET_IN_DESIRED_STATE`** — move the property out of
  `desired_state_schema` to the runtime binding, as shown above.
- **`W_ADDON_FAMILY_UNKNOWN`** — this one is deliberately a warning, not an
  error, and the fix isn't necessarily "change it." The known-family list
  lives in a released `gtdx` binary, while your `describe.json` is signed
  and immutable the moment you publish it. A platform release newer than
  your `gtdx` can recognize a family yours doesn't — erroring here would
  reject an addon a newer platform would happily match. Only rename the
  family if you actually meant an existing one and mistyped it; otherwise
  the warning is informational, telling you which platforms won't be able
  to match a flow's request against this addon yet.

As with views, `gtdx lint` works from the raw JSON and never deserializes
into the typed describe, so two invariants live at *parse* time instead —
enforced by `gtdx validate`, installation, and anything else in this SDK
that loads a `describe.json`, but invisible to `gtdx lint` alone: a
duplicate addon `id` (it would make one of the two unaddressable once
namespaced), and a duplicate output `name` within one addon (a binding would
resolve to whichever entry happened to be seen last). The same output name
reused *across* different addons is fine — outputs are scoped per addon.
Always run `gtdx validate` as well as `gtdx lint` before you ship.
