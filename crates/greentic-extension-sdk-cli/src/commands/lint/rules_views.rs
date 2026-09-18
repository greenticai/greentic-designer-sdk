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
pub(crate) const KNOWN_SLOTS: [&str; 3] =
    ["designer.sidebar", "admin.sidebar", "admin.tenantDetail"];

/// `src=` markers that count as a remote *asset* reference, checked only
/// inside a tag that actually fetches one (`<script>`, `<img>`).
const SRC_MARKERS: [&str; 4] = ["src=\"http", "src='http", "src=\"//", "src='//"];

/// `href=` markers, checked only inside a `<link>` tag. An `<a href="http…">`
/// is an ordinary hyperlink, not an asset the pack needs to vouch for, so it
/// must never trip this rule.
const HREF_MARKERS: [&str; 4] = ["href=\"http", "href='http", "href=\"//", "href='//"];

/// Tags whose `src` attribute names a fetched asset. Deliberately does not
/// include every element that can carry `src` (e.g. `<a>` has none, and
/// `<iframe src>` is a different, not-yet-covered case) — this is a
/// best-effort lint for the obvious mistake, not an exhaustive HTML audit.
const SRC_ASSET_TAGS: [&str; 2] = ["script", "img"];

/// The tag name at the start of a tag body, i.e. the text between `<` and the
/// next `>` with any leading `/` (a closing tag) stripped. `<script src=...`
/// yields `script`; `</script>` also yields `script` but callers only ever
/// look at opening tags found by scanning attribute markers, so that is
/// harmless.
fn tag_name(tag_body: &str) -> &str {
    tag_body
        .trim_start_matches('/')
        .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .next()
        .unwrap_or("")
}

/// Scan `html` for a remote asset reference, scoped to the tag that owns the
/// attribute: `href` only counts inside a `<link>` tag, `src` only inside a
/// `<script>` or `<img>`-style tag. Deliberately a substring/tag scan and not
/// a real HTML parse — this is a lint meant to catch the obvious mistake, not
/// a security boundary. The real boundary is the CSP the host sets on the
/// asset route. Because it is a scan and not a parser, it assumes a `>` never
/// appears inside a quoted attribute value, same as the rest of this rule.
fn find_remote_asset_marker(html: &str) -> Option<&'static str> {
    let lowered = html.to_lowercase();
    for chunk in lowered.split('<').skip(1) {
        let Some(tag_end) = chunk.find('>') else {
            continue;
        };
        let tag_body = &chunk[..tag_end];
        let name = tag_name(tag_body);

        if name == "link" {
            if let Some(marker) = HREF_MARKERS.iter().find(|m| tag_body.contains(*m)) {
                return Some(marker);
            }
        } else if SRC_ASSET_TAGS.contains(&name)
            && let Some(marker) = SRC_MARKERS.iter().find(|m| tag_body.contains(*m))
        {
            return Some(marker);
        }
    }
    None
}

/// `^[a-z0-9][a-z0-9._-]*$` — the same pattern `describe-v2.json` declares
/// for `views[].id`. Enforced here too because nothing else on an author's
/// machine checks it: `TryFrom<DescribeJsonRaw>` doesn't, and `gtdx lint`
/// never runs schema validation. Left unchecked, this id is joined straight
/// into a filesystem path a few lines down.
/// `pub(crate)` so `gtdx new --view-id` applies the same rule at scaffold
/// time; a view id rejected here would otherwise only surface as
/// `E_VIEW_ID_PATTERN` on the author's first lint.
pub(crate) fn is_valid_view_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    id.bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'))
}

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

        // Checked first, before `id` is ever joined into a path: an id like
        // `../..` must be reported as an invalid id, not surfaced as a
        // confusing "entry file does not exist" once it has already steered
        // the lookup outside the view's own directory.
        if !is_valid_view_id(id) {
            out.push(Violation::error(
                "E_VIEW_ID_PATTERN",
                format!("view id {id:?} must match ^[a-z0-9][a-z0-9._-]*$"),
            ));
            continue;
        }

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
        if !path.exists() {
            out.push(Violation::error(
                "E_VIEW_ENTRY_MISSING",
                format!(
                    "view {id:?} declares entry {entry:?} but {} does not exist; \
                     a view whose HTML is missing is a broken install",
                    path.display()
                ),
            ));
            continue;
        }
        let Ok(html) = std::fs::read_to_string(&path) else {
            out.push(Violation::error(
                "E_VIEW_ENTRY_UNREADABLE",
                format!(
                    "view {id:?} entry {entry:?} exists at {} but could not be read \
                     (not valid UTF-8, or a permissions error) — this file was found, \
                     unlike a missing entry, but its contents could not be checked",
                    path.display()
                ),
            ));
            continue;
        };

        if let Some(marker) = find_remote_asset_marker(&html) {
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
