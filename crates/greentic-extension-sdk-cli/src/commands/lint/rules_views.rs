//! Lint rules for `contributions.views[]` — the checks that need the project
//! directory on disk, which neither the JSON Schema nor the `DescribeJson`
//! deserializer can perform. Structural cross-references (duplicate ids,
//! dangling tool names) live in the deserializer instead, so every consumer
//! gets them and not only authors who run lint.
//!
//! In its own module because `rules.rs` sits at 498 lines against a 500-line
//! budget — the same reason `rules_secret_key.rs` exists.

use std::path::Path;

use super::Violation;

/// Placement slots the hosts publish today.
///
/// This is a snapshot. Hosts serve the live list at `/api/views/slots`, and a
/// snapshot embedded in a released CLI goes stale by construction — which is
/// exactly why an unknown slot is a warning and never an error. An author on
/// an older `gtdx` must still be able to target a slot shipped last week.
pub(super) const KNOWN_SLOTS: [&str; 3] =
    ["designer.sidebar", "admin.sidebar", "admin.tenantDetail"];

/// Markers for a remote asset reference in an entry HTML. Deliberately a
/// substring scan and not an HTML parse: this is a lint meant to catch the
/// obvious mistake, not a security boundary. The real boundary is the CSP the
/// host sets on the asset route.
const REMOTE_MARKERS: [&str; 6] = [
    "src=\"http",
    "src='http",
    "src=\"//",
    "href=\"http",
    "href='http",
    "href=\"//",
];

pub(super) fn check_views(describe: &serde_json::Value, dir: &Path) -> Vec<Violation> {
    let Some(views) = describe
        .pointer("/contributions/views")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for view in views {
        let id = view
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("<unnamed>");

        if let Some(slot) = view.pointer("/placement/slot").and_then(|v| v.as_str())
            && !KNOWN_SLOTS.contains(&slot)
        {
            out.push(Violation::warning(
                "W_VIEW_SLOT_UNKNOWN",
                format!(
                    "view {id:?} targets unknown slot {slot:?}; known slots are {}. \
                     This list is a snapshot — if the host has since added the slot, \
                     ignore this warning.",
                    KNOWN_SLOTS.join(", ")
                ),
            ));
        }

        let Some(entry) = view.get("entry").and_then(|v| v.as_str()) else {
            continue;
        };

        if entry.starts_with('/') || entry.split('/').any(|seg| seg == "..") {
            out.push(Violation::error(
                "E_VIEW_ENTRY_PATH",
                format!(
                    "view {id:?} entry {entry:?} escapes assets/views/{id}/; \
                     entry must be a relative path inside the view's own directory"
                ),
            ));
            continue;
        }

        let path = dir.join("assets/views").join(id).join(entry);
        let Ok(html) = std::fs::read_to_string(&path) else {
            out.push(Violation::error(
                "E_VIEW_ENTRY_MISSING",
                format!(
                    "view {id:?} declares entry {entry:?} but {} does not exist; \
                     a view whose HTML is missing is a broken install",
                    path.display()
                ),
            ));
            continue;
        };

        let lowered = html.to_lowercase();
        if let Some(marker) = REMOTE_MARKERS.iter().find(|m| lowered.contains(*m)) {
            out.push(Violation::error(
                "E_VIEW_REMOTE_ASSET",
                format!(
                    "view {id:?} entry {entry:?} references a remote asset ({marker}…); \
                     assets must ship inside the pack, otherwise the manifest sha256 \
                     covers a file that then pulls unverified code at runtime"
                ),
            ));
        }
    }
    out
}
