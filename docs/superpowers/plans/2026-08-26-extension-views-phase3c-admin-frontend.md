# Extension Views — Phase 3c (Admin frontend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put extension-contributed views into the Admin console's navigation, render them safely, and give a tenant admin a place to arrange them.

**Architecture:** The server returns views already resolved and filtered, so the frontend holds no permission logic. Rendering is one catch-all route and one iframe host. Arranging is a cross-extension Navigation tab, not a per-extension list.

**Tech Stack:** React 18, react-router-dom v6, `@tanstack/react-query`, zustand, Tailwind, Vite.

**Spec:** `docs/superpowers/specs/2026-08-26-extension-views-phase3-admin-design.md` — read "Navigation" and "Placement UI" in full.

## Global Constraints

- Repo `greentic-designer-admin`, frontend under `web/`. Integration branch `develop`.
- **The frontend performs no permission filtering.** `GET /api/admin/views` returns what the caller may see. `web/src/lib/navItems.ts` carries a comment warning that its `tiers` and `capability` fields must be kept "in step" by hand with the Rust route classifier — two copies of one rule. Views get one copy, server-side. Do not add a second.
- No `allow-same-origin` on the iframe, ever. That single attribute is the whole security model.
- Conventional commits. Never run `git stash` — this machine shares stash stacks across sessions.

## Prerequisite and parallelism

Phase 3b provides `GET /api/admin/views`, `GET /api/views/slots`, the asset route, the bridge endpoints, and the `TenantScoped` placement routes. This plan can be built **in parallel** with 3b, because the response shapes are fixed in the spec and in 3b's Task 4 — that is what makes the parallelism safe. Build against those shapes; integrate when 3b lands.

A working reference already exists on branch `BimaPangestu28/extension-view-render`: `web/src/pages/ExtensionViewHost.tsx`, `web/src/lib/extensionViews.ts`, plus the `App.tsx` and `Sidebar.tsx` wiring. It was built as a demo over seeded data and its render path is verified end to end. **Port it; do not re-derive it.** Task 1 is largely adoption, not invention.

---

### Task 1: The view host and the catch-all route

**Files:**
- Create: `web/src/pages/ExtensionViewHost.tsx`, `web/src/lib/extensionViews.ts`
- Modify: `web/src/App.tsx`
- Test: `web/src/pages/__tests__/ExtensionViewHost.test.tsx` (follow whatever test setup the repo already uses; if there is none for components, say so and cover the bridge reducer as a pure unit instead)

**Interfaces:**
- Consumes: `GET /api/admin/views`, the asset route, the bridge endpoints.
- Produces: route `/x/:extId/:viewId`; hook `useAdminViews()`.

- [ ] **Step 1: Port the reference**

Take `ExtensionViewHost.tsx` and `extensionViews.ts` from `BimaPangestu28/extension-view-render`. Read them rather than copying blind — three properties must survive the port, and a test should pin each:

1. `sandbox="allow-scripts"` with **no** `allow-same-origin`.
2. The message listener checks `event.source === iframeRef.current.contentWindow`, never `event.origin`. Under an opaque origin `event.origin` is the literal string `"null"` and proves nothing.
3. The host posts with `targetOrigin: "*"` — an opaque origin cannot be named — which is exactly why nothing secret may ride in `init`.

- [ ] **Step 2: Write tests that would fail if any of the three regressed**

The third is the subtle one: a test that a message forged from a different window is ignored. If the repo has no component test harness, extract the message handling into a pure function and test that.

- [ ] **Step 3: Wire the route**

`<Route path="/x/:extId/:viewId" element={<ExtensionViewHost />} />` in `web/src/App.tsx`, inside the authenticated `AppFrame` wrapper.

- [ ] **Step 4: Run the frontend checks, then commit**

Whatever `web/package.json` exposes — typecheck, lint, tests, build. Paste real output.

---

### Task 2: Navigation merge

**Files:**
- Modify: `web/src/components/shell/Sidebar.tsx`, `web/src/lib/navItems.ts`
- Modify: `web/src/features/tenants/tabs/…` for the `admin.tenantDetail` slot
- Modify: `web/src/components/shell/SearchPalette.tsx`

**Interfaces:**
- Consumes: `useAdminViews()`.

- [ ] **Step 1: Merge into the sidebar by slot**

Views with `slot === "admin.sidebar"` merge into `navGroups()`. Honour `path` as the section to place under; where it does not resolve, fall back to an "Extensions" section **and surface a diagnostic** — never drop the view silently. That fallback is a contract promise made to extension authors, not a convenience.

Ordering: `order`, then extension id, then view id. Total and stable, so two extensions choosing the same number never swap between renders.

- [ ] **Step 2: Merge into tenant detail**

Views with `slot === "admin.tenantDetail"` merge into `TAB_GROUPS` in `TenantDetail.tsx`, rendered by the existing `GroupNavLayout`. The `/tenants/:id/:tab` route already supplies a tenant, so the bridge `init` carries `tenantId` with no extra machinery.

- [ ] **Step 3: Search palette**

Add views to the palette. Admin already maintains `SETTINGS_NAV` for exactly this reason: a page nobody can find effectively does not exist.

- [ ] **Step 4: Handle the degraded cases visibly**

A view whose assets failed to sync is omitted by the server with a diagnostic. Show it — do not render an empty iframe, and do not silently drop it either.

- [ ] **Step 5: Tests, then commit**

Pin the ordering rule and the unresolved-slot fallback. Both are the kind of behaviour that regresses without anyone noticing until an author complains.

---

### Task 3: The Navigation tab

**Files:**
- Create: `web/src/features/tenants/tabs/ViewNavigationTab.tsx` and its supporting hooks
- Modify: `TenantDetail.tsx` tab list

**Interfaces:**
- Consumes: the `TenantScoped` placement and team-override routes from 3b.

- [ ] **Step 1: Build it as a cross-extension surface**

One decision shapes this task: **placement is not arranged from a per-extension list.** `ExtensionsTab.tsx` handles enablement — "this extension, for this tenant: on or off" — and stays exactly as it is. Arranging navigation is a cross-extension task; you cannot order a section while looking at one extension.

So: a tree of the resolved navigation with every extension's views in place. Move between sections, reorder, toggle per view.

- [ ] **Step 2: The three states that need to be visible**

- A view **locked** by a platform admin shows a lock and the reason — not a control that quietly does nothing.
- A view whose **assets failed to sync** shows its diagnostic in place, so the person seeing it is the person who can act.
- **Team scoping** per view follows the `TeamMembersDialog` / `TeamsTab` pattern already used for extension overrides. Do not invent a second idiom for the same idea.

- [ ] **Step 3: Tests, then commit**

At minimum: a locked view's placement controls are disabled while its enable toggle is not — that is the rule most likely to be implemented backwards, because "locked" reads as "frozen entirely".

---

### Task 4: The gate

- [ ] **Step 1: Run everything**

The frontend checks in `web/package.json`, then the repo gate. Foreground, real output pasted.

- [ ] **Step 2: Verify end to end against a running Admin**

Not a build. Start the server, log in, and confirm: the view appears in the sidebar, its page renders in the iframe, the bridge answers `init`, a locked view cannot be moved, and a view can be hidden for one team and not another.

- [ ] **Step 3: PR against `develop`**

## What this plan deliberately leaves out

- **Slot editing beyond the two the host publishes.** `GET /api/views/slots` is the catalogue; the UI offers what it returns and nothing else.
- **Anything about `invokeTool`.** It is refused on this surface with a typed error; the UI surfaces that error and does not special-case it.
