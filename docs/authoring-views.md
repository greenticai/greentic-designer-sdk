# Authoring views

A view is a UI page your extension contributes to the Greentic Designer or the
Greentic Admin console. You write the HTML, JS and CSS; they ship inside your
`.gtxpack`; a future host release serves them and renders your entry in a
sandboxed iframe.

> **Phase 1 status — read this before you publish one.** Today's SDK lets you
> declare a view, scaffold one, lint it, schema-validate it and pack it into a
> signed `.gtxpack`. **No host renders views yet.** Everything below this
> point describes what a view *will* look like once a designer/admin release
> that understands `contributions.views[]` ships — that is a later phase, not
> this one. If you publish a view-bearing extension today, it packs and signs
> cleanly, but nothing displays it.
>
> **There is also a compat trap, and it is the more serious half of this
> note.** `Contributions` is `#[serde(deny_unknown_fields)]`. A designer built
> against a pre-views contract crate does not skip an unrecognised `views`
> key — it fails to parse your `describe.json` at all, so the whole extension
> refuses to load. Meanwhile `gtdx new` fills `compat.min_designer_version`
> from this SDK's `MIN_DESIGNER_VERSION` constant, currently `1.2.0`. So a
> view-bearing describe *claims* `>=1.2.0` while actually being unloadable on
> every designer from 1.2.0 through 1.2.8 — the entire released line as of
> this writing. Do not publish a `views[]`-carrying extension for general use
> until a host release that understands it has shipped; until then, treat
> `--with-view` as a way to develop and test your page locally, not to ship
> it.

Scaffold one:

    gtdx new my-ext --kind design --with-view

## What ships

    assets/views/<view-id>/index.html
    assets/views/<view-id>/bridge.js
    assets/views/<view-id>/app.js
    assets/views/<view-id>/style.css

`bridge.js` is the postMessage transport `index.html` loads before `app.js`
and that `app.js` calls into as `window.greentic`; the scaffold's `index.html`
hard-requires it, so pruning it from the list above breaks your own page.

The packer copies `assets/` verbatim, and `manifest.json` records a sha256 for
every file, so your page is tamper-evident without you doing anything.

Everything your page loads must ship in that directory. `gtdx lint` rejects a
remote `<script src>`/`<img src>` or `<link href>` in your **entry HTML**
with `E_VIEW_REMOTE_ASSET` (an ordinary `<a href>` hyperlink is fine — it
isn't fetched): the manifest hash would otherwise cover a file that pulls
unverified code at runtime. Note the scope: lint only scans the entry HTML
file itself. A remote reference built up inside `app.js` — a script tag
inserted at runtime, or `import()` of a remote URL — is not caught by this
rule.

`--with-view` also fills `views[].tools` for you: it takes the first tool in
the extension's own `contributions.tools`, whatever the chosen `--kind`
happens to contribute (`design` → `echo`, `llm` → `complete`, and so on). A
kind that contributes no tools (`deploy`, `provider`) gets `tools: []` — the
view still ships, it just can't call one yet. `--with-view` is rejected
outright for `--kind mcp`: `wasix:mcp/router` artifacts carry no
`contributions` block at all for a view to attach to.

## Declaring it

    "contributions": {
      "views": [{
        "id": "usage-dashboard",
        "surface": "admin",
        "title_key": "view.usage.label",
        "title_fallback": "Usage",
        "entry": "index.html",
        "placement": { "slot": "admin.tenantDetail", "path": ["access"], "order": 20 },
        "min_visibility": "tenant_admin",
        "tools": ["fetch_usage"]
      }]
    }

`placement` is a suggestion. Platform admins decide which tenants get your
extension at all, and tenant admins decide where your view actually lands and
which of their teams can see it. `min_visibility` is a floor, not a guarantee.

Known slots: `designer.sidebar`, `admin.sidebar`, `admin.tenantDetail`. An
unknown slot is a lint warning rather than an error, because this list is a
snapshot in your `gtdx` build and hosts add slots between releases. A host that
cannot resolve your placement mounts the view under an "Extensions" section and
records a diagnostic — it will not disappear on you.

## What your page can reach

Your page runs with an opaque origin. It has no host cookies, no
`localStorage`, no access to the parent DOM, and its own `fetch()` would send
`Origin: null`. Everything goes through the bridge, and the host holds the
credentials:

    await greentic.ready                                  // locale, theme, surface, context
    await greentic.invokeTool("fetch_usage", { days: 30 }) // your own tool
    await greentic.callApi("GET", "/api/flows")            // platform REST
    await greentic.fetch("https://api.example.com/x")      // proxied server-side

The last three are gated by `runtime.permissions.ui`:

    "permissions": {
      "ui": {
        "fetchHosts": ["https://api.example.com/*"],
        "platformApi": [{ "method": "GET", "path_pattern": "/api/flows" }]
      }
    }

Every bridge call, and the initial `init` handshake, times out after 10
seconds if the host never replies — `greentic.ready` rejects and a call
promise rejects with a "timed out" error rather than hanging forever. If your
page reports "Could not connect to the host", this is almost always why: no
host is currently listening for `postMessage` from this frame at all (see the
Phase 1 status note at the top of this document), not a slow network.

The host intersects that allowlist with the permissions of whoever is looking
at the page. Declaring `/api/admin/tenants/*` does not let an ordinary tenant
user read other tenants — the bridge can only ever narrow what that person
could already do by hand.

Never expect a secret to arrive in the browser. Ask the bridge for a result;
the credential stays on the server.

## Lint codes

| Code | Meaning |
|---|---|
| `E_VIEW_ID_PATTERN` | `id` does not match `^[a-z0-9][a-z0-9._-]*$` |
| `E_VIEW_ENTRY_MISSING` | `entry` names a file that is not in your project |
| `E_VIEW_ENTRY_PATH` | `entry` escapes `assets/views/<id>/` |
| `E_VIEW_ENTRY_UNREADABLE` | `entry` names a file that exists but couldn't be read (not UTF-8, or a permissions error) |
| `E_VIEW_REMOTE_ASSET` | the entry HTML has a remote `<script src>`/`<img src>` or `<link href>` |
| `W_VIEW_SLOT_UNKNOWN` | `placement.slot` is not in this `gtdx` build's snapshot |

Duplicate view ids and a `tools[]` entry naming a tool you do not contribute
are rejected when the describe is *parsed* — `gtdx validate` and installation
both do this and so reject them, and so does anything else in this SDK that
loads a `describe.json`. **`gtdx lint` does not**: it works from the raw JSON
and never deserializes into the typed describe, so it stays silent on both of
these. Always run `gtdx validate` as well as `gtdx lint` before you ship.
