# Environments and Addons — Declarative Deployment Design

> **Status (2026-08-26): DESIGN, not started.** No code written. This spec
> defines the target architecture for Designer environments and the
> `AddonExtension` kind.
>
> **Blocked on, and must not ship before:**
> - **Production trust root** — `root_verifier.rs:377` ships
>   `PROD_ROOT_PUBKEY_B64 = ""`, so `from_embedded()` always fails. This is
>   the same item tracked as *"D.5+ trust root + key custody"* in
>   `2026-05-13-extensions-1.0-cleanup.md`, still marked **BLOCKED on org
>   decision**. Third-party addons are a materially higher-value target than
>   design extensions (see §9.2), so this blocker binds harder here.
> - **Contract release `greentic:extension-base@0.3.0`** — adding a variant to
>   the WIT `enum kind` is a breaking change and must be coordinated with the
>   commercial runtime repo (§9.2).
> - **Legal sign-off on the BYOC engine.** Terraform is BUSL-1.1; running it on
>   customers' behalf in a PaaS is plausibly outside its additional use grant.
>   The spec plans on OpenTofu (D12), but counsel must confirm before
>   engineering time is committed to either.
>
> **Prerequisites that are independently shippable and should land first:** §9.1.

---

## 1. Problem

Designer environments are intended to become a Railway-like surface: a user
picks an environment, adds services and addons (Qdrant, Redis, Postgres),
wires them together, and deploys. Two things block that today.

**There is no environment concept.** `grep -ri environment` across the SDK
returns only env-var prose. Environments are new construction, not an
extension of something existing.

**The deploy contract has the wrong shape for it.** `wit/extension-deploy.wit`
models a one-shot push: `deploy(target_id, artifact_bytes, credentials,
config) -> job`, `poll(job_id)`, `rollback(job_id)`. There is no stack, no
dependency graph, no desired state, and no reconciliation. It answers "send
this artifact somewhere", which is a different verb from "keep this set of
things existing and correct".

The specific question this spec answers: **how do we support addons whose
deployment is complex — an addon that needs configuration and custom
code, e.g. Qdrant with collections defined, or Redis with ACLs and a
non-stock image?**

## 2. Decisions

Each decision below was taken deliberately; the rationale matters more than
the choice, because the rationale is what a future reader needs in order to
know whether a changed circumstance invalidates it.

| # | Decision | Rationale |
|---|---|---|
| D1 | Deployment targets both Greentic-hosted infra and customer-owned cloud (BYOC). | Product requirement. Hosted serves onboarding; BYOC serves customers with data-residency obligations. |
| D2 | **Extensions never emit Terraform HCL.** They emit engine-neutral desired state. | D1 makes HCL-as-contract wrong in both directions: on hosted it forces `terraform apply` to create workloads in our own cluster, and it puts third-party Terraform providers inside our credential boundary, outside the WASM sandbox that the rest of the security model depends on. Terraform-family IaC stays, demoted to one renderer on the BYOC path — OpenTofu, see D12 and §6.2. |
| D3 | The addon contract is **always declarative**. Tiering applies to who reconciles, never to the contract shape. | A "run this script" escape hatch cannot be observed, diffed, or made idempotent. Tiering the contract instead of the implementation would produce two contracts and rot the weaker one. |
| D4 | First-party addons (Qdrant, Redis, Postgres) **must** be `AddonExtension`s using the same public interface as third-party ones. No privileged path. | An interface its author never uses decays. This repo already carries that scar: `v1-extensions-are-not-a-v2-reference`. Dogfooding is the only mechanism that keeps the public path working. |
| D5 | Environments are **independent**; only `artifact_ref` is promoted between them. Config and desired state stay per-environment. | Chosen over base+override inheritance. Inheritance requires provenance UI ("why is this value what it is") to stay comprehensible; the simpler model defers that cost until there is evidence it is needed. |
| D6 | Addons declare the workload they need; **the platform provisions it**. Addons do not provision. | This is the single split that makes one contract serve both D1 paths. It also keeps "custom image / sidecar" a set of fields rather than a code path. |
| D7 | `render-workload` and `plan` are **pure functions**. | Independently: (a) purity makes the hard logic table-testable, and (b) it is the *only* thing testable in this repo at all — there is no WASM execution harness (§8). The design landed on the right side of that line by accident; recording it here so it is not undone by accident. |
| D8 | Cross-resource references are restricted to `${resources.<id>.outputs.<name>}`, not a general template language. | Buys three properties that a template language forfeits: a parseable DAG for ordering and cycle detection, and — the important one — sensitive outputs that resolve to *secret references* rather than values (§4.3). |
| D9 | Rollback is **re-applying a previous spec revision**, not an addon-implemented undo. | Correct `undo` is close to unwritable by an addon author. Level-triggered reconciliation makes it unnecessary: a previous revision is just another desired state. |
| D10 | `plan` returns **planned desired state plus replace-paths**, never a list of opaque actions. | Terraform's `PlanResourceChange` returns `planned_state` + `requires_replace`, and core derives the verb; the provider never names an action. An opaque action payload cannot be diffed, rendered, or consistency-checked by the platform — only the plugin can say what will happen, which makes the plan unverifiable. Since both `current` and `planned` are JSON documents of a schema the addon published, the platform can diff them generically. |
| D11 | The host **enforces plan/apply consistency, fatally, with no escape hatch.** Every known leaf in the planned state must equal the corresponding leaf observed after apply. | This is what separates a plan from a dry run. Terraform enforces it via `objchange.AssertObjectCompatible` and emits "Provider produced inconsistent result after apply". Its one escape hatch, `legacy_type_system`, is a decade-old compatibility wart its own proto comments shout at people not to use. We are greenfield and must not ship one. |
| D12 | The BYOC renderer targets **OpenTofu**, not Terraform. | Terraform moved to BUSL-1.1 in August 2023 (now under IBM); the additional use grant excludes embedding or hosting it to build an offering competitive with HashiCorp's commercial products. A PaaS that runs Terraform on customers' behalf sits squarely against HCP Terraform. OpenTofu is the MPL-2.0 Linux Foundation continuation. **Needs counsel to confirm, but plan on OpenTofu.** |
| D13 | The contract carries a **`deferred` signal** (`absent-prereq`, `config-unknown`) from day one. | A three-call interface has no way to say "I cannot plan this yet". Terraform had to bolt `Deferred` on later and negotiate it behind a capability flag. We hit this immediately: day-2 config cannot be planned before the instance exists. |

### 2.1 Non-goals

- **Preview/ephemeral environments per PR.** Deferred. Nothing here forecloses
  them; they need addon seed/snapshot performance work and a TTL/cost story.
- **Glue code in the flow runtime** (e.g. an embed→upsert pipeline against
  Qdrant). That is a node/tool that happens to consume an addon binding, and
  belongs to the design-extension surface, not here.
- **Arbitrary lifecycle hooks.** Explicitly rejected: unobservable,
  non-idempotent, and security-hostile. D3 is the alternative.
- **Config inheritance between environments.** See D5.

## 3. Architecture overview

```
Project
└── Environment { id, name, placement, resources[] }
      placement := Hosted { region } | Byoc { cloud, credential_ref }
      │
      ├── Resource::Service { artifact_ref, bindings }      ← promoted between envs
      └── Resource::Addon   { addon_ref, config, desired_state }
                                    │
                                    ├── workload      → Renderer (§6)
                                    └── desired_state → Reconciler (§7)
```

`placement` is the only field the renderer consults to choose a materialization
path. One data model, two engines.

## 4. Environment and resource model

### 4.1 Resources

A **Service** is a Greentic deployable — a flow bundle or runner. It carries
`artifact_ref`, which is the sole thing `promote` copies.

An **Addon** is an instance of a resource type offered by an
`AddonExtension`. It carries `addon_ref` (extension id + resource id +
version), `config` (validated against `config-schema`), and `desired_state`
(validated against `desired-state-schema`).

### 4.2 Spec, status, revisions

Per resource the control plane stores:

- `spec` — desired state. Every change creates a new **immutable revision**.
- `status` — last `observe` result plus conditions (§7.4).
- `engine_handle` — opaque renderer handle (K8s field-manager scope, or
  Terraform state address).

Revisions are what make D9 work. Rollback selects an older revision and
re-enters the normal reconcile loop; nothing special-cases it.

### 4.3 Bindings

Addons declare typed `outputs`. References use exactly one form:

```
${resources.<resource_id>.outputs.<name>}
```

Two consequences, both load-bearing:

**Ordering and cycles.** The reference set parses into a DAG. Provisioning
order is derived, not declared; cycles are rejected at validation time.

**Sensitive outputs never become values.** An output marked `sensitive`
resolves to a *secret reference*, not a string. The renderer emits
`valueFrom.secretKeyRef` (Kubernetes) or a `sensitive` variable wired to the
resource attribute (Terraform). A Redis password therefore never enters the
control plane's plan document, so it cannot surface in a plan UI, an audit
log, or a support bundle.

A general template language forfeits both. `${addon.pg.outputs.url}/mydb` is
therefore **not** supported; an addon that wants a per-database URL exposes it
as its own output.

### 4.4 Promote

`promote(from_env, to_env, service_id)` copies `artifact_ref` only.

If the destination environment lacks a resource that the artifact's bindings
require, promote **fails before anything runs**, and reports the diff. Failing
in the middle of a production apply is the outcome this rule exists to
prevent.

## 5. The addon contract

New WIT package `greentic:extension-addon@0.1.0`, new `ExtensionKind::Addon`.

### 5.1 Interfaces

```wit
package greentic:extension-addon@0.1.0;

interface resources {
  record output-spec {
    name: string,
    output-type: output-kind,     // text | number | boolean
    sensitive: bool,
    description: string,
  }

  record resource-spec {
    id: string,                   // "qdrant"
    family: string,               // "vector-db" — flows may require a family, not a vendor
    display-name: string,
    description: string,
    icon-path: option<string>,
    config-schema: string,        // JSON Schema: user-facing knobs
    desired-state-schema: string, // JSON Schema: day-2 state
    outputs: list<output-spec>,
    supports-backup: bool,
  }

  list-resources: func() -> list<resource-spec>;
  validate-config: func(id: string, config-json: string) -> list<diagnostic>;
  validate-desired-state: func(id: string, desired-json: string) -> list<diagnostic>;
}

interface workload {
  variant workload-spec {
    container(container-workload),   // primary + sidecars + volumes + readiness
    managed(managed-workload),       // { service-class, params-json } — see §5.3
  }

  /// PURE. No network, no side effects. Called at plan time, cacheable.
  render-workload: func(id: string, config-json: string)
    -> result<workload-spec, extension-error>;
}

interface reconciler {
  /// Why the addon could not produce a plan yet. Mirrors Terraform's
  /// `Deferred`, present from v0.1.0 rather than retrofitted (D13).
  enum deferred-reason {
    absent-prereq,     // the instance is not up; ask again after readiness
    config-unknown,    // a referenced output is not resolved yet
  }

  variant plan-outcome {
    planned(planned-change),
    deferred(deferred-reason),
  }

  record planned-change {
    /// The addon's own desired-state JSON, amended with any defaults the
    /// addon knows. The platform diffs `current-json` against this to render
    /// the human-readable plan — the addon never names an action (D10).
    planned-json: string,
    /// JSON Pointer paths whose change cannot be applied in place and
    /// require destroy-and-recreate. This is what surfaces as destructive
    /// in the plan UI and what gates approval.
    requires-replace: list<string>,
  }

  enum outcome { applied, failed-retryable, failed-terminal }
  record apply-report {
    /// State actually reached. The host asserts every known leaf of
    /// `planned-json` is present and equal here, and fails the apply if not
    /// (D11).
    observed-json: string,
    outcome: outcome,
    message: string,
  }

  /// Observe the live instance. Deliberately does NOT receive desired state:
  /// an observer that can see intent will reconcile toward it, and drift
  /// detection becomes unfalsifiable. (Terraform withholds config from
  /// `ReadResource` for exactly this reason.)
  observe: func(id: string, binding: binding) -> result<string, extension-error>;

  /// PURE. No network, no side effects.
  plan: func(id: string, current-json: string, desired-json: string)
    -> result<plan-outcome, extension-error>;

  apply: func(id: string, binding: binding,
              current-json: string, planned-json: string)
    -> result<apply-report, extension-error>;
}
```

### 5.2 Why this shape

**`render-workload` and `plan` are pure** (D7). An addon's hard logic is a
table test: feed current and desired, assert the planned state. No cluster, no
Qdrant, no fixtures beyond JSON. §8 explains why this is not merely convenient
but necessary.

**`plan` returns state, not actions** (D10). This is the correction that
matters most. An action list with an opaque `payload-json` would mean the
platform cannot say what an apply will do — only the addon could, and the
platform would have to take its word. Because `current-json` and
`planned-json` are both documents of a schema the addon published, the
platform diffs them generically and renders the plan itself. `requires-replace`
is what makes a change destructive, and it is a list of paths the platform can
read, not a boolean the addon asserts.

This is also the provisioner lesson in contract form: an imperative payload the
system cannot model is precisely what HashiCorp regrets shipping (§5.5).

**`apply` returns the state it reached**, and the host asserts compatibility
with the plan (D11). Without that assertion the plan is a dry run — a
suggestion, not a contract.

**`failed-retryable` vs `failed-terminal`** is the addon's call. A connection
reset and a schema violation are different classes and the platform cannot
distinguish them from outside.

**`deferred` exists from the start** (D13). Day-2 config genuinely cannot be
planned before the instance is reachable, and "I cannot plan this yet" must be
a first-class answer rather than an error or a lie.

### 5.3 The `managed` variant

`container` cannot express a cloud-managed service (RDS, Azure Cache for
Redis). The `managed` variant carries `service-class` plus opaque params.

The BYOC renderer maps it to the cloud resource. The hosted renderer **refuses
it at plan time with an explicit message** — never at apply time, and never by
silently substituting a container.

### 5.4 Protocol access for `observe` / `apply`

Qdrant day-2 config is REST, which the existing `http` host import covers.
Redis is RESP and is not reachable from the current sandbox.

The permission field `network` cannot carry this: it is a URL-prefix glob and
`publish/validator.rs:42-59` rejects anything that is not `https://` (loopback
excepted). A new permission field is required.

**Proposal:** an `endpoints` permission (`host:port` form) plus a host
`socket` import **scoped to the resource's own binding** — the host refuses
any connect target not present in the binding handed to that call. This is
narrow by construction: a reconciler only ever talks to the instance it
reconciles. It also avoids growing the WIT surface once per protocol.

This field **must** be added to the consent prompt. `prompt.rs:311-313`
currently surfaces only `network`, `secrets`, and `callExtensionKinds`;
`llmRoles` and `oauthProviders` are already invisible to users, and a new
capability of this weight must not join them.

Note the tension worth recording: WASI security guidance prefers granting
`wasi:http/outgoing-handler` over `wasi:sockets/tcp`, because HTTP permits
host-side URL filtering that raw TCP cannot. RESP forces sockets for the Redis
family. Binding-scoped egress is the mitigation, and Spin's
`allowed_outbound_hosts` is the reference implementation of the idea.

**Prior-art check, and it is sobering.** "Create a Qdrant collection" has
essentially no IaC prior art: the official `qdrant/qdrant-cloud` Terraform
provider stops at the cluster boundary (accounts, clusters, keys, backups,
RBAC) and ships no collection resource; the only provider that manages
collections has 54 downloads. The official Qdrant operator exposes
`QdrantCluster` and no collection CRD. "Create a Redis ACL user" has strong
prior art — `RedisLabs/rediscloud` (5.3M downloads) and the Redis Enterprise
operator's `RedisEnterpriseUser` / `REACL` CRDs — but **exclusively against
vendor control-plane REST APIs, never against RESP**. Managing ACLs on a
self-hosted Redis declaratively is unsolved.

We would be first. That is the opportunity and the risk, and it should be
sized honestly rather than assumed routine.

### 5.5 Rejected alternatives

**Script-shaped actions / lifecycle hooks.** This is Terraform provisioners,
and HashiCorp's own guidance is the argument: *"Terraform cannot model the
actions of provisioners as part of a plan because they can in principle take
any action."* Provisioners are not idempotent (creation-time provisioners run
only at creation and never converge), are not recorded in state, and on
failure taint the resource so the only recovery is destroy-and-recreate —
*"because Terraform cannot reason about what the provisioner does"*. If
`apply` accepts anything script-shaped, we have rebuilt provisioners.
(`https://developer.hashicorp.com/terraform/language/v1.11.x/resources/provisioners/syntax#provisioners-are-a-last-resort`
— the canonical text was removed in the 1.12 docs restructure, so cite the
versioned URL.)

**An OSB-style `provision` / `update` / `deprovision` contract.** This is the
uncomfortable one, and it deserves recording rather than burying. The two most
successful PaaS addon contracts in existence — the Heroku Add-on Partner API
and the Open Service Broker API — have **no `observe`, no `plan`, and no drift
detection**. They are at-least-once, idempotent-on-UUID, async via `202` plus
polling. If most addons only ever need "provision me a Redis, here is the
URL", a reconciler contract makes every author pay Terraform's complexity tax
for Heroku's problem.

We reject it because we are solving a harder problem in two specific ways OSB
does not cover: BYOC, where the platform does not own the substrate, and day-2
configuration, which OSB genuinely cannot express. But D6 is what makes this
affordable — because the platform provisions the workload, a simple addon's
entire implementation is `render-workload` returning a container spec and a
`plan` that returns its input unchanged. The simple case stays simple without a
second contract. **If that stops being true in practice, this decision should
be revisited, not defended.**

**Crossplane's two-boolean observation.** `ExternalObservation` collapses the
plan into `ResourceExists` / `ResourceUpToDate`, with `Diff` explicitly a
debug-level human string. The documented consequence is that Crossplane has no
`terraform plan` equivalent and users get no feedback before changes land.
That is what this contract becomes if the plan step is dropped.

## 6. Renderers

One platform-side trait (ordinary Rust, not WASM): `plan` / `apply` /
`observe` / `destroy`, over the resolved environment graph.

The addon's `render-workload` output is engine-neutral; the renderer maps it.
A field one engine cannot express surfaces **at plan time**.

### 6.1 Hosted — server-side apply, not Helm

Helm's value is packaging, release history, and rollback. Revisions (§4.2)
already provide all three, so Helm would add a templating layer over specs
that are already typed. Server-side apply gives declarative convergence, field
ownership, and conflict detection directly.

### 6.2 BYOC — generate OpenTofu JSON, not HCL

**Engine: OpenTofu, not Terraform** (D12). Terraform is BUSL-1.1 under IBM
since August 2023; its additional use grant excludes embedding or hosting it
to build an offering competitive with HashiCorp's commercial products, and a
PaaS running it on customers' behalf sits directly opposite HCP Terraform.
OpenTofu is the MPL-2.0 Linux Foundation fork. **Confirm with counsel before
committing engineering time either way.** OpenTofu additionally has state
encryption at rest, which Terraform lacks and which matters because state
holds secrets in plaintext otherwise.

The configuration format accepts JSON syntax natively. Since it is generated,
generating `.tf.json` eliminates string templating and its quoting and
injection failure modes outright.

One workspace per environment; backend and locking owned by us; each apply
runs in an isolated worker holding only that tenant's credentials.

**Serialize plans behind applies, not just applies behind applies.** A plan is
only meaningful relative to a state version, so a plan queued while an apply
is in flight is already stale. HCP Terraform's documented behaviour is the
model: *"If there's already a run in progress, the new run won't start until
the current one has completely finished — HCP Terraform won't even plan the
run yet, because the current run might change what a future run would do."*

**Two limits this path inherits and the hosted path does not.** First,
provider configuration must be fully known at plan time, so "create the
instance, then configure inside it" cannot happen in one apply — this is
`hashicorp/terraform#30937`, unsolved. Second, the runner must reach the data
plane, which for a private-subnet database means a private runner or a tunnel.
Both are why §7.1 orders infra before day-2 and why D13's `absent-prereq`
exists. The in-cluster operator model that the hosted path can use dissolves
both problems; BYOC cannot, and we should say so to customers rather than
discover it with them.

## 7. Reconciliation

### 7.1 Level-triggered

`observe` → compare against the spec revision → `plan` → gate → `apply` →
record status. The loop never remembers "which step it was on"; it re-derives
everything from observed state.

Two layers, ordered: infra converges and readiness passes **first**, then
day-2 — day-2 requires a live endpoint. Before readiness, `plan` legitimately
answers `deferred(absent-prereq)` and the platform re-queues rather than
erroring.

The reconcile entry point takes a **resource identifier, not a change
description**. Watches and events are optimizations that say "go re-read this";
they are never the source of truth. Kubernetes states the rule plainly: *"you
can't count on having seen it turn from false to true, only that you now
observe it being true."* Work is serialized per resource instance — no two
workers reconcile the same addon concurrently.

### 7.2 Plan/apply consistency

After `apply`, the host compares `apply-report.observed-json` against the
`planned-json` it approved. **Every known leaf in the plan must be present and
equal in the result.** A mismatch fails the apply and is reported as an addon
defect, not a user error.

This is not optional polish. Without it, the plan the user approved has no
relationship to what happened, and "plan" is a marketing word for "dry run".
Terraform enforces exactly this (`objchange.AssertObjectCompatible`) and its
error text names the plugin as the bug: *"Provider produced inconsistent
result after apply … This is a bug in the provider."*

**No escape hatch** (D11). Terraform's `legacy_type_system` flag downgrades the
check to a log line and its own proto comments read `==== DO NOT USE THIS ====`.
A greenfield contract that ships one will have it used.

### 7.3 Partial failure

The platform re-observes and re-plans from actual state. A partial failure is
therefore **not a stuck state machine — it is simply a new current state.**

This is what D9 buys and why addons need no compensation logic. Roll forward,
or select an earlier revision; both re-enter the same loop. Errors must always
re-queue with rate-limited backoff — an error path that silently drops the item
means that resource is never retried.

Deprovisioning external infrastructure needs a finalizer-style guard: the
resource record must survive until cleanup succeeds, or a customer's Qdrant
cluster outlives the record that knows about it.

### 7.4 Status and drift

Status is a **list of conditions keyed by type**, each with `status`
(`True`/`False`/`Unknown`), a required CamelCase `reason`, a human `message`,
`lastTransitionTime`, and `observedGeneration` so a client can tell "this
reflects the spec I submitted" from "this is stale". Start with `Provisioned`,
`Ready`, `Configured`, `Degraded`.

**Not a `phase` enum.** Kubernetes deprecated that pattern explicitly:
*"Phase was essentially a state-machine enumeration field, that contradicted
system-design principles and hampered evolution, since adding new enum values
breaks backward compatibility."* In an ecosystem where third parties write the
reconcilers, an additive condition vocabulary is the only design that lets an
addon report something we did not anticipate. Condition names describe observed
state — adjectives or past-tense verbs (`Ready`, `Failed`), never present-tense
(`Deploying`).

Drift is detectable precisely because `observe` returns full current state. A
collection dropped by hand appears as a diff, not as a mystery. Note the cost
Crossplane pays for continuous reconciliation — every managed resource polls
its API forever — so drift-check cadence must be a per-environment policy, not
a constant.

Destroying a resource that holds data requires a snapshot when
`supports-backup` is true, or an explicit force. Changes touching a
`requires-replace` path need approval unless the environment is marked
auto-approve.

## 8. Testing

Constrained by a hard fact: **this workspace has no WASM execution harness.**
`wasmtime` appears in no `Cargo.toml`, no `Cargo.lock`, and no `.rs` file.
There is no `Linker`, no `Store`, and no host stub for
`greentic:extension-host/*`. The supported path is `cargo component build` to
materialize `src/bindings.rs`, then `cargo test` calling the guest impls as
ordinary Rust traits (`AGENTS.md.tmpl:41-63`).

| Surface | How |
|---|---|
| `render-workload`, `plan` | Host-side table tests in the scaffold. Pure, so this is complete coverage of the logic. |
| `observe`, `apply` | Dependency injection. `MockHttpClient::restrict_to_hosts` seeded from `describe.runtime.permissions`. The SDK will not wire mocks into bindgen free functions (`AGENTS.md.tmpl:73-76`). |
| Renderers | Platform-side snapshot tests: `EnvironmentSpec` → expected `.tf.json` and K8s manifests. |

### 8.1 Conformance suite

Ship in `greentic-extension-sdk-testing` so every addon inherits it:

- **`plan(x, x)` must return `planned(x)` with an empty `requires-replace`.**
  One property, checked mechanically, for every addon, with no infrastructure.
  It catches non-idempotent reconcilers — the failure mode this whole design
  exists to prevent — at `cargo test` time.
- **Plan stability:** `plan(current, desired)` called twice on the same inputs
  must produce identical output. A plan that varies cannot be approved.
- **Plan/apply consistency** (D11), as a harness rather than a unit test: given
  a recorded `current` and `planned`, assert every known leaf of `planned`
  appears in `apply`'s `observed-json`. This is the same assertion the host
  makes in production, so an addon that passes locally passes there.
- Round-trip: `plan → apply → observe` must equal desired.

## 9. Prerequisites

### 9.1 Independently shippable, should land first

These are bug fixes with standalone value. They also make the new kind
markedly cheaper, because each one is a place a sixth kind would otherwise be
silently dropped.

**Five hand-written kind lists are already stale today**, before any new kind
exists — the `hardcoded-kind-lists-rot` pattern, recurring:

| Location | Current defect |
|---|---|
| `cli/src/commands/install.rs:154-159` | `warn_if_designer_cannot_load` omits `Provider` |
| `cli/src/commands/search.rs:26-32` | `--kind provider` answers "unknown kind" |
| `cli/src/commands/lint/rules.rs:96-104` | `kind_dir_name()` re-implements `dir_name()`, omits `wasix:mcp/router` — so `W_DESCRIBE_DIFF_BREAKING` silently skips MCP routers |
| `cli/src/commands/info.rs:41-47` | hand-written candidate list |
| `cli/src/commands/list.rs:51-57` | hand-written vec for `KindArg::All` |

All five must derive from `ExtensionKind::ALL`, which already exists for this
purpose and is used correctly by `uninstall`, `doctor`, `enable`, `disable`,
`outdated`, and `update`.

**Two silent-failure fallbacks must become hard errors:**

- `cli/src/scaffold/template.rs:103` — `_ => Vec::new()`: a missing template
  arm scaffolds zero kind files and reports success.
- `cli/src/commands/new/mod.rs:587` — `_ => "extension-misc"`: a new WIT file
  lands in a dependency directory no `Cargo.toml.tmpl` references.

**Schema/enum drift must be closed:**

- `contract/schemas/describe-v2.json:25-32` hand-maintains the `kind` enum. A
  new variant compiles in Rust but fails every `gtdx validate` and `gtdx
  publish`. Generate it, or add a test that forces both to agree.
- `permissions` has no `additionalProperties: false`. This is why
  `oauthProviders` validates while being absent from the schema entirely —
  and it means a typo'd permission passes silently today.

### 9.2 Release gates — must not ship without

**Production trust root.** `root_verifier.rs:377` is empty, so `--trust
strict` is anchored by a prior TOFU pin rather than a certificate chain;
`verify.rs:69-73` admits this in a warning. A design extension under this
weakness is a moderate risk. An addon reconciler holds credentials to freshly
provisioned infrastructure and, on the BYOC path, runs against the customer's
own cloud account — a different class of target. **A third-party addon
marketplace cannot open on an empty trust root.** Tracked as D.5+ in
`2026-05-13-extensions-1.0-cleanup.md`, blocked on an org decision.

**Contract release `extension-base@0.3.0`.** `wit/extension-base.wit:10-15`
declares `enum kind { design, bundle, deploy, provider }`. Additive enum
variants are breaking in WIT — as `extension-design.wit` states explicitly in
its `target-kind` comment. Adding `addon` bumps `extension-base`, and because
`manifest.get-identity()` references `types.kind`, `manifest` bumps with it,
which reaches every existing world. The runtime must serve `manifest@0.2.0`
and `@0.3.0` concurrently during migration. This is cross-repo coordination
and must be planned as a contract release, not folded into a feature branch.

`wasix:mcp/router` avoids the WIT enum by importing no Greentic WIT at all.
Addons cannot use that escape: they need `diagnostic` and `extension-error`
from `extension-base/types`.

## 10. Cost of the new kind

Roughly 40 touch points across five crates. Beyond §9.1, the notable ones:

- `contract/src/kind.rs:27-34` — `ALL: [Self; 5]`, hardcoded length.
- `contract/src/describe/mod.rs:128-133` — `execution` is rejected unless
  `kind == Bundle`; decide whether addons may carry it.
- `cli/src/scaffold/mod.rs:10-19` — scaffold-side `Kind` enum, distinct from
  `ExtensionKind` and with more variants.
- `cli/src/scaffold/template.rs:7-18` — one `include_dir!` static per kind.
- `cli/src/scaffold/embedded.rs:56-70` — `files_for_kind` derives
  `extension-addon.wit` by name; `:127-138` asserts `wit_files().len() == 8`.
- `cli/src/commands/new/mod.rs:239-246` — `WIT_VERSION_PLACEHOLDERS` needs
  `("wit_version_addon", "addon")` or rendering fails on an unsubstituted
  placeholder.
- `cli/src/commands/new/wizard.rs:17-33` — `KIND_CHOICES`; a kind absent here
  is unreachable interactively.
- `cli/tests/contract_version_consistency.rs:68-111` — asserts the expected
  version map covers exactly the `.wit` files on disk.
- `cli/tests/cli_new/scaffold_kinds.rs:429-456` and `:481-499` — enumerate
  kinds literally; the template must ship a working example and a test
  containing `is_err()`.
- `registry/src/lifecycle.rs:119-132` — `Provider` triggers
  `post_install_provider`; decide whether addons need an analogue. Note that
  uninstall (`lifecycle.rs:159-166`) prunes neither the state entry nor the
  provider gtpack — do not replicate that leak.

## 11. Open questions

1. May an `AddonExtension` carry `contributions`? Addons plausibly contribute
   node types (a "Qdrant search" node bound to the addon), which would couple
   this kind to the design surface.
2. Does the hosted renderer expose a resource-quota model per environment, and
   does the addon declare requests/limits, or does the platform impose them?
3. Snapshot/restore is declared via `supports-backup` but has no interface
   yet. Is backup a reconciler action, or a separate interface?
4. Should `family` be a closed vocabulary? An open string lets two addons claim
   `vector-db` with incompatible outputs, which defeats the point of flows
   requiring a family rather than a vendor.
5. **Write-only fields in desired state.** A password in `desired-state` cannot
   be read back by `observe`, so it can never be shown consistent and will
   diff forever. Terraform needed both a protocol capability flag and a
   dedicated plan-time check for this; `rediscloud_acl_user` sidesteps it by
   forcing replacement on any password change. Decide before v0.1.0 whether
   `desired-state-schema` marks fields write-only, or whether secrets are
   excluded from desired state entirely and injected via `binding`.
6. **Which of the RPCs beyond observe/plan/apply do we need at v0.1.0?**
   Terraform's lifecycle needs ten, and the extra ones are not decoration:
   schema migration when an addon version changes its `desired-state-schema`,
   importing a resource created outside the platform, renaming without
   destroy/recreate, and validate-before-touch. Every one is a problem this
   system will have. Shipping only three means retrofitting the rest.
7. **When does the OSB rejection get revisited?** §5.5 rejects an
   OSB/Heroku-shaped contract on the argument that D6 keeps the simple case
   cheap. That claim is testable: if the first five first-party addons need
   substantial `plan` logic for what is really "provision and hand back a
   URL", the argument has failed and a simple tier is the honest answer.
