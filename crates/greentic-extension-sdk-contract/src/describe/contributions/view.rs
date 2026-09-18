//! `View` — a UI page an extension contributes to a Greentic host surface.
//!
//! The page's HTML/JS/CSS ship inside the `.gtxpack` under
//! `assets/views/<id>/`; the host serves them and renders the entry in a
//! sandboxed iframe with an opaque origin. Everything the page is allowed to
//! reach is declared in `runtime.permissions.ui`, not here — a reviewer reads
//! one grant block rather than one per view.

use serde::{Deserialize, Serialize};

/// Which host application the view targets. A view that belongs in both
/// declares two entries: placement differs per surface anyway, so a single
/// entry could never carry both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Designer,
    Admin,
}

/// Floor on who may see the view at all. Deliberately three values rather
/// than mirroring either host's vocabulary (the Designer speaks
/// `role`/`is_operator`, the Admin speaks tiers plus capabilities): the
/// author's declaration is only a floor, and the operative gate is tenant and
/// team configuration held by the host.
///
/// Host mapping is fixed, not author-chosen: `Member` is any authenticated
/// user of the tenant, `TenantAdmin` is Admin tier `tenant` and above (which
/// includes `partnership`), `PlatformAdmin` is Admin tier `platform` or
/// `is_operator` in the Designer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Member,
    TenantAdmin,
    PlatformAdmin,
}

/// Where the author suggests the view appears. Every configuration layer may
/// override it, so this is a default and not a demand.
///
/// `slot` and `path` are strings rather than a closed enum on purpose. The
/// hosts' tab sets are hand-written arrays that change with the product, while
/// `describe.json` is signed and immutable once published — a closed enum in a
/// signed artifact rots exactly the way this project's hard-coded kind lists
/// did. The safety net is behavioural: `gtdx lint` warns on an unknown slot,
/// and a host that cannot resolve a placement mounts the view under an
/// "Extensions" section with a diagnostic rather than dropping it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    /// Host-defined mount point: `designer.sidebar`, `admin.sidebar`,
    /// `admin.tenantDetail`.
    pub slot: String,
    /// Section/group path under the slot. Empty means the top level of the
    /// slot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
    /// Sort hint within the parent. Hosts break ties by extension id then view
    /// id, so ordering is total and stable even when two extensions pick the
    /// same number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    /// Unique within the extension. The host namespaces it as
    /// `<extension_id>/<id>`.
    pub id: String,
    pub surface: Surface,
    /// Key resolved against the top-level `localization` block.
    pub title_key: String,
    /// Literal shown when `title_key` has no entry for the active locale.
    pub title_fallback: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Entry HTML, relative to `assets/views/<id>/` inside the pack.
    pub entry: String,
    pub placement: Placement,
    #[serde(default, skip_serializing_if = "is_default_visibility")]
    pub min_visibility: Visibility,
    /// Names of this extension's own contributed tools the view may invoke
    /// through the host bridge. Every name must appear in
    /// `contributions.tools[].name`; the deserializer enforces that.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

// `skip_serializing_if` is called by serde_derive as `path(&self.field)`, so
// the reference parameter is mandatory even though `Visibility` is `Copy`.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default_visibility(v: &Visibility) -> bool {
    matches!(v, Visibility::Member)
}
