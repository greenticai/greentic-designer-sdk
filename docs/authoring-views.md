# Authoring views

A view is a UI page your extension contributes to the Greentic Designer or the
Greentic Admin console. You write the HTML, JS and CSS; they ship inside your
`.gtxpack`; the host serves them and renders your entry in a sandboxed iframe.

Scaffold one:

    gtdx new my-ext --kind design --with-view

## What ships

    assets/views/<view-id>/index.html
    assets/views/<view-id>/app.js
    assets/views/<view-id>/style.css

The packer copies `assets/` verbatim, and `manifest.json` records a sha256 for
every file, so your page is tamper-evident without you doing anything.

Everything your page loads must ship in that directory. `gtdx lint` rejects a
remote `<script>` or `<link>` with `E_VIEW_REMOTE_ASSET`: the manifest hash
would otherwise cover a file that pulls unverified code at runtime.

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

The host intersects that allowlist with the permissions of whoever is looking
at the page. Declaring `/api/admin/tenants/*` does not let an ordinary tenant
user read other tenants — the bridge can only ever narrow what that person
could already do by hand.

Never expect a secret to arrive in the browser. Ask the bridge for a result;
the credential stays on the server.

## Lint codes

| Code | Meaning |
|---|---|
| `E_VIEW_ENTRY_MISSING` | `entry` names a file that is not in your project |
| `E_VIEW_ENTRY_PATH` | `entry` escapes `assets/views/<id>/` |
| `E_VIEW_REMOTE_ASSET` | the entry HTML pulls a remote script or stylesheet |
| `W_VIEW_SLOT_UNKNOWN` | `placement.slot` is not in this `gtdx` build's snapshot |

Duplicate view ids and a `tools[]` entry naming a tool you do not contribute
are rejected when the describe is parsed, so they fail `gtdx validate` and
installation as well as lint.
