# Authoring capabilities

Everything an extension declares about itself — what it may reach, what it
offers, how much memory it may use, what it shows — is set at scaffold time by
`gtdx new`, either through the interactive wizard or through flags. This
document is the reference for both.

## "Capability" means five different things

The word is overloaded in `describe.json`, and the flags keep the five apart
rather than flattening them. Reading this table first will save you looking for
`views` under `capabilities`:

| What you want | Where it lives | Flags |
|---|---|---|
| Contracts this extension **provides to** or **needs from** others | `capabilities.offered[]` / `capabilities.required[]` | `--offer-capability`, `--require-capability` |
| What the **WASM guest** may reach | `runtime.permissions.*` | `--permit-network`, `--permit-secret`, `--permit-call-kind`, `--permit-llm-role`, `--permit-oauth` |
| The guest's **memory ceiling** | `runtime.memoryLimitMB` | `--memory-mb` |
| A **UI page** the extension contributes, and what that page may reach | `contributions.views[]` + `runtime.permissions.ui` | `--with-view` and the `--view-*` family |
| Where a **tool** may be invoked from | `contributions.tools[].capabilities[]` | `--tool-capability` |

A sixth thing shares the name but is not authored here: `resource-request`
(`min-memory-mb`, `min-cpu-millis`) in `wit/extension-addon.wit` is what an
*addon workload* asks the platform for, and has nothing to do with the guest's
own `memoryLimitMB`.

## The wizard

Run `gtdx new` with no name on a terminal. After the usual name / kind / id /
version / author / license questions it asks one multi-select:

```
? Capabilities (space to toggle, enter to continue, none is fine)
  [ ] Network access         hosts the guest may fetch (runtime.permissions.network)
  [ ] Secrets                secret read grants (runtime.permissions.secrets)
  [ ] Call other extensions  kinds it may call into (callExtensionKinds)
  [ ] LLM roles              LLM roles it may request from the host
  [ ] OAuth providers        OAuth providers it may request tokens for
  [ ] Memory limit           runtime.memoryLimitMB — 1..=1024, default 64
  [ ] Offered capabilities   capability contracts it provides to others
  [ ] Required capabilities  capability contracts it needs from others
  [ ] Contributed view       a UI page, its placement and what it may reach
  [ ] Tool surfaces          show tools in flows, the agentic worker, or both
  [ ] Icon                   metadata.icon — svg/png/jpg/webp, <= 1 MiB
  [ ] Catalogue metadata     summary, description, homepage, repository, keywords
```

Only the rows you check are drilled into, so an extension that needs none of
this is still one keystroke past the picker. List-valued rows prompt repeatedly
until you submit an empty line.

Rows that cannot apply are not offered: **Contributed view** is absent for
`--kind mcp` (a `wasix:mcp/router` artifact has no `contributions` block for a
view to attach to), and **Tool surfaces** is absent for any kind whose scaffold
contributes no tools — today `bundle`, `deploy`, `provider`, `addon`,
`wasm-component` and `mcp`. That list is read from each kind's own template, so
it follows the templates rather than needing an edit here.

Flags you pass on the command line pre-check their row and seed its prompts.
Unchecking a row clears what the flag supplied: the picker is your final say.

## The flags

### Runtime limits

    --memory-mb <MB>

`runtime.memoryLimitMB`, bounded `1..=1024` by both the JSON Schema and the
Rust deserializer. Every scaffold now writes this field explicitly at its
default of `64` — the value is unchanged, but it is no longer invisible.

### Host permissions

    --permit-network <PATTERN>     repeatable
    --permit-secret <GRANT>        repeatable
    --permit-call-kind <KIND>      repeatable
    --permit-llm-role <ROLE>       repeatable
    --permit-oauth <PROVIDER>      repeatable

`--permit-network` must be `https://`. Plain `http://` is accepted only for
loopback hosts (`localhost`, `127.0.0.1`, `[::1]`), matching what the extension
runtime actually honours — it drops non-loopback `http://` patterns, so a
cleartext entry to a public host would be an allowlist that looks deliberate
and does nothing.

`--permit-secret` takes **grants**, not credential field names: `*`, a URI
containing `://` (`secret://acme/`), or a path prefix ending in `/` (`acme/`).
A field name an operator must supply belongs in `requiredSecrets` — see
[`authoring-secrets.md`](./authoring-secrets.md).

`--permit-call-kind` is an open list in the schema. An unrecognised value is
kept and reported as a note, not rejected.

### Capability contracts

    --offer-capability <ID@VERSION>   repeatable
    --require-capability <ID@REQ>     repeatable

An id is `<namespace>:<path>`, e.g. `greentic:guardrail/topic`. The version is
split on the **last** `@`, so a path containing one survives.

`--offer-capability` requires an **exact** version (`1.0.0`): it is what other
extensions resolve against, and `gtdx publish` rejects a range there.
`--require-capability` takes a semver requirement (`^1`, `>=1.2, <2`).

An id given to both is refused — an extension cannot depend on a capability it
provides itself (`E_CAP_CYCLE`).

### Contributed view

    --with-view
    --view-id <ID>                    default: hello
    --view-surface <designer|admin>   default: designer
    --view-slot <SLOT>                default: <surface>.sidebar
    --view-title <TEXT>               default: the view id, humanised
    --view-min-visibility <member|tenant_admin|platform_admin>
    --view-fetch-host <PATTERN>       repeatable
    --view-api "<METHOD> <PATH>"      repeatable

The page is scaffolded under `assets/views/<view-id>/`, matching the id the
describe records. A `--view-*` flag without `--with-view` is an error rather
than a silently ignored value.

`--view-fetch-host` follows the same address rule as `--permit-network`.
`--view-api` is a method (`GET`, `POST`, `PUT`, `PATCH`, `DELETE`) and a path
pattern starting with `/`.

An unknown `--view-slot` is a **warning**, never an error: the known-slot list
is a snapshot in your `gtdx` build and hosts add slots between releases.

See [`authoring-views.md`](./authoring-views.md) for what the sandboxed page
can and cannot reach, and read its Phase 1 status note before publishing one.

### Tool surfaces

    --tool-capability <flow|agentic_worker>   repeatable

Sets `contributions.tools[].capabilities[]` on every tool the scaffold
contributes. Both values together mean the tool is usable as a flow node *and*
callable by the agentic worker. A tool that declares nothing is treated by
consumers as `["flow"]`.

Declaring `agentic_worker` also writes `tools[].agentic_worker_metadata` with
the conservative defaults the planning layer already assumes for a tool that
ships none — `side_effects: external`, `confirmation_required: true`,
`cost: medium` — so the assumption is visible in the file you edit rather than
implied by its absence. Metadata a template already authored is never
overwritten. Loosen the values once you know the tool's real behaviour.

The flag is refused for a kind that contributes no tools: there is nowhere to
record the surface, and writing it where nothing reads it would be worse than
saying so.

### Icon and catalogue metadata

    --icon <PATH>            svg/png/jpg/jpeg/webp, <= 1 MiB
    --summary <TEXT>
    --description <TEXT>
    --homepage <URL>
    --repository <URL>
    --keyword <KEYWORD>      repeatable

`--icon` copies the file to `assets/icon.<ext>` and sets `metadata.icon` to
that pack-relative path, removing any stale sibling in another format. The
same code backs `gtdx publish --icon`.

## What gets written, and what does not

A field the kind's template already writes keeps its shape and is filled in
place: `permissions.network`, `.secrets`, `.callExtensionKinds`,
`capabilities.offered`, `.required` and `memoryLimitMB` are present in every
scaffold whether you configure them or not.

A field no template writes appears **only** when you ask for it: `llmRoles`,
`oauthProviders`, `permissions.ui`, `contributions.views`,
`tools[].capabilities`, and the optional `metadata.*` entries. An unconfigured
scaffold is byte-for-byte what the templates render.

List values are **appended**, not replaced. This matters on the
`--from-openapi` path, where `runtime.permissions.network` already carries the
hosts derived from the spec's `servers` block: `--permit-network` widens that
allowlist rather than erasing it.

## Validation happens at scaffold time

Every rule above is one `gtdx lint` or `gtdx publish` would apply later, so a
project these flags accept passes its own first lint. The errors name the lint
code they pre-empt:

| Input | Rule | Pre-empts |
|---|---|---|
| `--permit-network`, `--view-fetch-host` | https, or http on loopback | `gtdx publish` |
| `--permit-secret` | must be a grant, not a plain key | `E_PERMS_SECRETS_PLAIN_KEY` |
| `--offer-capability` | exact semver | `gtdx publish` |
| `--offer-capability` + `--require-capability` | no self-cycle | `E_CAP_CYCLE` |
| `--memory-mb` | `1..=1024` | schema + deserializer |
| `--view-id` | `^[a-z0-9][a-z0-9._-]*$` | `E_VIEW_ID_PATTERN` |
| `--view-slot` | known slot (warning only) | `W_VIEW_SLOT_UNKNOWN` |
