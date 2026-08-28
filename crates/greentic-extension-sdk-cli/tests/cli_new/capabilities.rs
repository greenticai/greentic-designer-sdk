//! `gtdx new` capability flags: every declaration an extension can make about
//! what it needs, offers and shows, set from the command line.
//!
//! The point of these tests is that a project the flags accept is one that
//! passes its own first `gtdx lint` and `gtdx validate` — the flags apply the
//! same rules those commands do, so a mistake is reported while the author is
//! still typing rather than after the scaffold exists.

use std::path::Path;
use std::process::Command;

use crate::fixtures::{all_kind_strs, gtdx_bin, run};

/// Scaffold into a temp dir with `extra` appended to a minimal `gtdx new`.
fn scaffold(name: &str, extra: &[&str]) -> (bool, String, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join(name);
    let mut cmd = Command::new(gtdx_bin());
    cmd.arg("new")
        .arg(name)
        .arg("-y")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target)
        .args(extra);
    let (ok, out, err) = run(&mut cmd);
    (ok, format!("{out}\n{err}"), tmp)
}

fn describe_at(dir: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(dir.join("describe.json")).expect("read describe"))
        .expect("parse describe")
}

/// The scaffold must survive the checks it will face — a describe that lints
/// and validates is the only evidence the flags wrote something legal.
fn assert_lints_and_validates(target: &Path) {
    let (lint_ok, out, err) = run(Command::new(gtdx_bin())
        .arg("lint")
        .arg("--dir")
        .arg(target));
    assert!(lint_ok, "scaffold must lint clean:\n{out}\n{err}");
    let (validate_ok, out, err) = run(Command::new(gtdx_bin()).arg("validate").arg(target));
    assert!(
        validate_ok,
        "scaffold must validate against the schema:\n{out}\n{err}"
    );
}

// ---------------------------------------------------------------------------
// The full surface, in one scaffold
// ---------------------------------------------------------------------------

#[test]
fn every_capability_flag_lands_in_the_describe() {
    let (ok, output, tmp) = scaffold(
        "capable",
        &[
            "--memory-mb",
            "128",
            "--permit-network",
            "https://api.acme.com/*",
            "--permit-secret",
            "secret://acme/",
            "--permit-call-kind",
            "ProviderExtension",
            "--permit-llm-role",
            "sorla_composer",
            "--permit-oauth",
            "hubspot",
            "--offer-capability",
            "greentic:guardrail/topic@1.0.0",
            "--require-capability",
            "greentic:llm/chat@^1",
            "--tool-capability",
            "flow",
            "--tool-capability",
            "agentic_worker",
            "--with-view",
            "--view-id",
            "usage",
            "--view-surface",
            "admin",
            "--view-slot",
            "admin.tenantDetail",
            "--view-title",
            "Usage",
            "--view-min-visibility",
            "tenant_admin",
            "--view-fetch-host",
            "https://api.acme.com/*",
            "--view-api",
            "GET /api/flows",
            "--summary",
            "Acme connector.",
            "--description",
            "Long-form description.",
            "--homepage",
            "https://acme.example",
            "--repository",
            "https://github.com/acme/ext",
            "--keyword",
            "acme",
            "--keyword",
            "crm",
        ],
    );
    assert!(ok, "scaffold failed:\n{output}");
    let target = tmp.path().join("capable");
    let d = describe_at(&target);

    assert_eq!(d["runtime"]["memoryLimitMB"], 128);

    let perms = &d["runtime"]["permissions"];
    assert_eq!(perms["network"][0], "https://api.acme.com/*");
    assert_eq!(perms["secrets"][0], "secret://acme/");
    assert_eq!(perms["callExtensionKinds"][0], "ProviderExtension");
    assert_eq!(perms["llmRoles"][0], "sorla_composer");
    assert_eq!(perms["oauthProviders"][0], "hubspot");
    assert_eq!(perms["ui"]["fetchHosts"][0], "https://api.acme.com/*");
    assert_eq!(perms["ui"]["platformApi"][0]["method"], "GET");
    assert_eq!(perms["ui"]["platformApi"][0]["path_pattern"], "/api/flows");

    assert_eq!(
        d["capabilities"]["offered"][0]["id"],
        "greentic:guardrail/topic"
    );
    assert_eq!(d["capabilities"]["offered"][0]["version"], "1.0.0");
    assert_eq!(d["capabilities"]["required"][0]["id"], "greentic:llm/chat");
    assert_eq!(d["capabilities"]["required"][0]["version"], "^1");

    let view = &d["contributions"]["views"][0];
    assert_eq!(view["id"], "usage");
    assert_eq!(view["surface"], "admin");
    assert_eq!(view["placement"]["slot"], "admin.tenantDetail");
    assert_eq!(view["title_fallback"], "Usage");
    assert_eq!(view["min_visibility"], "tenant_admin");
    assert!(
        target.join("assets/views/usage/index.html").exists(),
        "the page must be scaffolded under the id the author chose"
    );

    let tool = &d["contributions"]["tools"][0];
    assert_eq!(tool["capabilities"][0], "flow");
    assert_eq!(tool["capabilities"][1], "agentic_worker");

    let meta = &d["metadata"];
    assert_eq!(meta["summary"], "Acme connector.");
    assert_eq!(meta["description"], "Long-form description.");
    assert_eq!(meta["homepage"], "https://acme.example");
    assert_eq!(meta["repository"], "https://github.com/acme/ext");
    assert_eq!(meta["keywords"][0], "acme");
    assert_eq!(meta["keywords"][1], "crm");

    assert_lints_and_validates(&target);
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

/// The default was invisible before: no template wrote it, so an author had no
/// way to discover the field existed at all.
#[test]
fn every_kind_scaffolds_an_explicit_memory_limit() {
    for kind in all_kind_strs() {
        let (ok, output, tmp) = scaffold("memtest", &["--kind", &kind]);
        assert!(ok, "kind {kind} scaffold failed:\n{output}");
        let d = describe_at(&tmp.path().join("memtest"));
        assert_eq!(
            d["runtime"]["memoryLimitMB"], 64,
            "kind {kind} must declare its memory limit explicitly"
        );
    }
}

#[test]
fn memory_outside_the_contract_bound_is_refused() {
    for mb in ["0", "2048"] {
        let (ok, output, _tmp) = scaffold("memtest", &["--memory-mb", mb]);
        assert!(!ok, "--memory-mb {mb} should be refused:\n{output}");
        assert!(output.contains("1..=1024"), "{output}");
    }
}

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

/// Cleartext to a public host is rejected by `gtdx publish` and dropped by the
/// runtime, so accepting it here would scaffold an allowlist that looks
/// deliberate and silently does nothing.
#[test]
fn cleartext_network_to_a_public_host_is_refused() {
    let (ok, output, _tmp) = scaffold("nettest", &["--permit-network", "http://api.acme.com/*"]);
    assert!(
        !ok,
        "plain http to a public host should be refused:\n{output}"
    );
    assert!(output.contains("--permit-network"), "{output}");
}

#[test]
fn loopback_http_is_allowed_for_local_development() {
    let (ok, output, tmp) = scaffold("nettest", &["--permit-network", "http://127.0.0.1:8787/*"]);
    assert!(ok, "loopback http must be accepted:\n{output}");
    let d = describe_at(&tmp.path().join("nettest"));
    assert_eq!(
        d["runtime"]["permissions"]["network"][0],
        "http://127.0.0.1:8787/*"
    );
}

/// `permissions.secrets` holds grants; a credential field name belongs in
/// `requiredSecrets`. Catching it here pre-empts `E_PERMS_SECRETS_PLAIN_KEY`.
#[test]
fn a_plain_secret_key_is_refused_with_the_field_that_wants_it() {
    let (ok, output, _tmp) = scaffold("sectest", &["--permit-secret", "SLACK_BOT_TOKEN"]);
    assert!(!ok, "a plain key should be refused:\n{output}");
    assert!(output.contains("requiredSecrets"), "{output}");
}

// ---------------------------------------------------------------------------
// Capability contracts
// ---------------------------------------------------------------------------

#[test]
fn offering_and_requiring_the_same_capability_is_refused() {
    let (ok, output, _tmp) = scaffold(
        "captest",
        &[
            "--offer-capability",
            "greentic:guardrail/topic@1.0.0",
            "--require-capability",
            "greentic:guardrail/topic@^1",
        ],
    );
    assert!(!ok, "a self-cycle should be refused:\n{output}");
    assert!(output.contains("E_CAP_CYCLE"), "{output}");
}

#[test]
fn an_offered_capability_must_pin_an_exact_version() {
    let (ok, output, _tmp) = scaffold(
        "captest",
        &["--offer-capability", "greentic:guardrail/topic@^1"],
    );
    assert!(!ok, "a range should be refused for offered:\n{output}");
    assert!(output.contains("exact semver"), "{output}");
}

#[test]
fn a_malformed_capability_ref_is_refused() {
    for entry in ["greentic:llm/chat", "chat@^1", "greentic:llm/chat@"] {
        let (ok, output, _tmp) = scaffold("captest", &["--require-capability", entry]);
        assert!(!ok, "{entry} should be refused:\n{output}");
    }
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

/// An accepted flag that lands nowhere is worse than a rejected one: the author
/// would believe the view was configured.
#[test]
fn view_flags_without_with_view_are_refused() {
    for flags in [
        vec!["--view-id", "usage"],
        vec!["--view-slot", "admin.sidebar"],
        vec!["--view-api", "GET /api/flows"],
        vec!["--view-fetch-host", "https://a.example/*"],
        vec!["--view-title", "Usage"],
    ] {
        let (ok, output, _tmp) = scaffold("viewtest", &flags);
        assert!(!ok, "{flags:?} should require --with-view:\n{output}");
        assert!(output.contains("--with-view"), "{flags:?}: {output}");
    }
}

/// Defaults must not trip the slot warning, or every plain `--with-view`
/// scaffold would start with a diagnostic it did nothing to earn.
#[test]
fn a_plain_with_view_scaffold_uses_a_known_slot_and_stays_quiet() {
    let (ok, output, tmp) = scaffold("viewtest", &["--with-view"]);
    assert!(ok, "scaffold failed:\n{output}");
    assert!(
        !output.contains("W_VIEW_SLOT_UNKNOWN"),
        "the default slot must be a known one:\n{output}"
    );
    let d = describe_at(&tmp.path().join("viewtest"));
    assert_eq!(
        d["contributions"]["views"][0]["placement"]["slot"],
        "designer.sidebar"
    );
    assert_eq!(
        d["contributions"]["views"][0]["title_fallback"], "Hello",
        "the default id must still yield the title it always did"
    );
}

/// The title follows the id unless the author names one, so a configured view
/// is not left calling itself "Hello".
#[test]
fn a_named_view_gets_a_title_derived_from_its_id() {
    let (ok, output, tmp) = scaffold("viewtest", &["--with-view", "--view-id", "usage-dashboard"]);
    assert!(ok, "scaffold failed:\n{output}");
    let d = describe_at(&tmp.path().join("viewtest"));
    assert_eq!(
        d["contributions"]["views"][0]["title_fallback"],
        "Usage Dashboard"
    );
    assert_eq!(
        d["contributions"]["views"][0]["title_key"],
        "view.usage-dashboard.label"
    );
}

/// An unknown slot is a warning, never an error: the known-slot list is a
/// snapshot in this binary and hosts add slots between releases.
#[test]
fn an_unknown_view_slot_warns_but_still_scaffolds() {
    let (ok, output, tmp) = scaffold(
        "viewtest",
        &["--with-view", "--view-slot", "admin.somethingNew"],
    );
    assert!(ok, "an unknown slot must not fail the scaffold:\n{output}");
    assert!(output.contains("W_VIEW_SLOT_UNKNOWN"), "{output}");
    let d = describe_at(&tmp.path().join("viewtest"));
    assert_eq!(
        d["contributions"]["views"][0]["placement"]["slot"],
        "admin.somethingNew"
    );
}

#[test]
fn a_view_id_that_could_escape_its_directory_is_refused() {
    let (ok, output, _tmp) = scaffold("viewtest", &["--with-view", "--view-id", "../../etc"]);
    assert!(!ok, "path traversal should be refused:\n{output}");
    assert!(output.contains("E_VIEW_ID_PATTERN"), "{output}");
}

#[test]
fn a_malformed_view_api_grant_is_refused() {
    for entry in ["/api/flows", "FETCH /api/flows", "GET api/flows"] {
        let (ok, output, _tmp) = scaffold("viewtest", &["--with-view", "--view-api", entry]);
        assert!(!ok, "{entry} should be refused:\n{output}");
        assert!(output.contains("--view-api"), "{entry}: {output}");
    }
}

// ---------------------------------------------------------------------------
// Tool surfaces
// ---------------------------------------------------------------------------

/// A tool declaring `agentic_worker` with no metadata is treated by the
/// planning layer as external and confirmation-requiring; writing those
/// defaults makes the assumption visible in the file the author edits.
#[test]
fn agentic_worker_tools_ship_conservative_metadata() {
    let (ok, output, tmp) = scaffold("tooltest", &["--tool-capability", "agentic_worker"]);
    assert!(ok, "scaffold failed:\n{output}");
    let d = describe_at(&tmp.path().join("tooltest"));
    let tool = &d["contributions"]["tools"][0];
    assert_eq!(tool["capabilities"][0], "agentic_worker");

    let metadata: serde_json::Value = serde_json::from_str(
        tool["agentic_worker_metadata"]
            .as_str()
            .expect("a JSON string"),
    )
    .expect("metadata is JSON");
    assert_eq!(metadata["side_effects"], "external");
    assert_eq!(metadata["confirmation_required"], true);
    assert_eq!(metadata["cost"], "medium");

    assert_lints_and_validates(&tmp.path().join("tooltest"));
}

/// `flow` is what a consumer assumes for a tool with no declaration, so it
/// needs no agentic-worker metadata.
#[test]
fn flow_only_tools_ship_no_agentic_worker_metadata() {
    let (ok, output, tmp) = scaffold("tooltest", &["--tool-capability", "flow"]);
    assert!(ok, "scaffold failed:\n{output}");
    let d = describe_at(&tmp.path().join("tooltest"));
    assert!(
        d["contributions"]["tools"][0]
            .get("agentic_worker_metadata")
            .is_none(),
        "{}",
        d["contributions"]["tools"][0]
    );
}

/// The surface lives on `contributions.tools[]`; a kind with none has nowhere
/// to record it, and saying so beats writing a flag nothing reads.
#[test]
fn a_tool_surface_on_a_toolless_kind_is_refused() {
    let (ok, output, _tmp) = scaffold(
        "tooltest",
        &["--kind", "deploy", "--tool-capability", "flow"],
    );
    assert!(!ok, "deploy contributes no tools:\n{output}");
    assert!(output.contains("contributes none"), "{output}");
}

// ---------------------------------------------------------------------------
// Doing nothing
// ---------------------------------------------------------------------------

/// A scaffold nobody configured must be exactly what the templates render.
/// The capability layer reads and rewrites `describe.json` only when it has
/// something to write, and this is what pins that.
#[test]
fn an_unconfigured_scaffold_matches_its_template_bytes() {
    let (ok, output, tmp) = scaffold("plain", &[]);
    assert!(ok, "scaffold failed:\n{output}");
    let target = tmp.path().join("plain");
    let d = describe_at(&target);

    assert_eq!(
        d["capabilities"]["offered"].as_array().map(Vec::len),
        Some(0)
    );
    assert_eq!(
        d["runtime"]["permissions"]["network"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    assert!(
        d["runtime"]["permissions"].get("llmRoles").is_none(),
        "an unset optional permission must stay absent, not appear empty"
    );
    assert!(d["runtime"]["permissions"].get("ui").is_none());
    assert!(d["contributions"].get("views").is_none());
    assert!(d["metadata"].get("keywords").is_none());
    assert!(
        d["contributions"]["tools"][0].get("capabilities").is_none(),
        "an unset tool surface must stay absent so consumers apply their own default"
    );

    assert_lints_and_validates(&target);
}
