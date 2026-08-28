//! `gtdx new --with-view` must produce a project that lints and validates on
//! the first run. A scaffold that emits an empty page teaches nothing — the
//! same lesson 1.2.7 and 1.2.8 already paid for on the other kinds.

use std::process::Command;

use crate::fixtures::{gtdx_bin, run};

#[test]
fn scaffold_with_view_produces_a_lintable_project() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("viewy");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("viewy")
        .arg("--kind")
        .arg("design")
        .arg("--with-view")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(ok, "scaffold failed: {err}");

    let entry = target.join("assets/views/hello/index.html");
    assert!(
        entry.exists(),
        "example page must exist at {}",
        entry.display()
    );
    assert!(target.join("assets/views/hello/bridge.js").exists());
    assert!(target.join("assets/views/hello/app.js").exists());

    let describe: serde_json::Value = serde_json::from_slice(
        &std::fs::read(target.join("describe.json")).expect("read describe"),
    )
    .expect("parse describe");
    let views = describe["contributions"]["views"]
        .as_array()
        .expect("views array");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0]["id"], "hello");
    assert_eq!(views[0]["entry"], "index.html");
    assert!(
        describe["runtime"]["permissions"]["ui"].is_object(),
        "a scaffolded view must come with its permissions.ui block"
    );

    let (lint_ok, _o, lint_err) = run(Command::new(gtdx_bin())
        .arg("lint")
        .arg("--dir")
        .arg(&target));
    assert!(
        lint_ok,
        "a fresh --with-view scaffold must lint clean: {lint_err}"
    );

    // The `design` kind is the one every doc points readers at
    // (`gtdx new my-ext --kind design --with-view`), so it is the path most
    // deserving of a real schema-validate pass, not just lint. `llm` and
    // `deploy` already get one; `design` did not.
    let (validate_ok, _o, validate_err) =
        run(Command::new(gtdx_bin()).arg("validate").arg(&target));
    assert!(
        validate_ok,
        "a fresh design --with-view scaffold must validate clean: {validate_err}"
    );

    // `design`'s `echo` tool declares `input_schema` with `required:
    // ["message"]` (matching the guest's own descriptor in
    // src/lib.rs.tmpl), so the derived placeholder args must not be the
    // empty object `const ARGS = {};` — that was the flagship scaffold path
    // emitting an unusable example call.
    let app_js = std::fs::read_to_string(target.join("assets/views/hello/app.js"))
        .expect("read scaffolded app.js");
    assert!(
        app_js.contains("\"message\""),
        "app.js must send the argument echo's own input_schema requires (`message`), \
         not an empty ARGS object: {app_js}"
    );
    assert!(
        !app_js.contains("const ARGS = {};"),
        "the flagship --kind design --with-view scaffold must not emit empty args: {app_js}"
    );
}

#[test]
fn scaffold_without_the_flag_ships_no_view() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("plain");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("plain")
        .arg("--kind")
        .arg("design")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(ok, "scaffold failed: {err}");

    assert!(
        !target.join("assets").exists(),
        "no view means no assets dir"
    );
    let describe: serde_json::Value = serde_json::from_slice(
        &std::fs::read(target.join("describe.json")).expect("read describe"),
    )
    .expect("parse describe");
    assert!(describe["contributions"].get("views").is_none());
}

/// The scaffold must derive the view's tool name from whatever the chosen
/// `--kind` actually contributes, not hardcode `echo`: `--kind llm`
/// contributes a tool named `complete`, and the patched describe must name
/// that, or the deserializer invariant (every `views[].tools` entry must
/// appear in `contributions.tools[].name`) rejects the whole document.
#[test]
fn scaffold_with_view_names_the_kinds_actual_tool_not_echo() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("llmy");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("llmy")
        .arg("--kind")
        .arg("llm")
        .arg("--with-view")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(ok, "scaffold failed: {err}");

    let describe: serde_json::Value = serde_json::from_slice(
        &std::fs::read(target.join("describe.json")).expect("read describe"),
    )
    .expect("parse describe");
    let views = describe["contributions"]["views"]
        .as_array()
        .expect("views array");
    assert_eq!(views.len(), 1);
    assert_eq!(
        views[0]["tools"][0], "complete",
        "an llm scaffold's view must name the tool llm actually contributes, not echo: {}",
        describe["contributions"]["views"][0]
    );

    let (validate_ok, _o, validate_err) =
        run(Command::new(gtdx_bin()).arg("validate").arg(&target));
    assert!(
        validate_ok,
        "a fresh --with-view scaffold must validate clean: {validate_err}"
    );
}

/// A kind with no contributed tools at all (`deploy`, `provider`) must not
/// scaffold a dangling `views[].tools` reference. The view simply references
/// no tool, which is valid and means the example page can't call one yet.
///
/// The field is *absent* rather than `[]`: the view is serialized through the
/// contract's own `View`, whose `tools` is `skip_serializing_if =
/// "Vec::is_empty"`, so an empty list has no wire form. Absent and `[]` decode
/// identically; what matters is that no name appears that
/// `contributions.tools` does not back.
#[test]
fn scaffold_with_view_for_a_toolless_kind_references_no_tool() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("deployy");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("deployy")
        .arg("--kind")
        .arg("deploy")
        .arg("--with-view")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(ok, "scaffold failed: {err}");

    let describe: serde_json::Value = serde_json::from_slice(
        &std::fs::read(target.join("describe.json")).expect("read describe"),
    )
    .expect("parse describe");
    let views = describe["contributions"]["views"]
        .as_array()
        .expect("views array");
    assert_eq!(views.len(), 1);
    assert_eq!(
        views[0]
            .get("tools")
            .and_then(|t| t.as_array())
            .map_or(0, Vec::len),
        0,
        "a toolless kind must reference no tool, not a dangling one: {}",
        describe["contributions"]["views"][0]
    );

    let (validate_ok, _o, validate_err) =
        run(Command::new(gtdx_bin()).arg("validate").arg(&target));
    assert!(
        validate_ok,
        "a fresh --with-view scaffold must validate clean even with no contributed tools: {validate_err}"
    );

    let (lint_ok, _o, lint_err) = run(Command::new(gtdx_bin())
        .arg("lint")
        .arg("--dir")
        .arg(&target));
    assert!(
        lint_ok,
        "a fresh --with-view scaffold must lint clean even with no contributed tools: {lint_err}"
    );
}

/// The scaffolded `app.js` must call the kind's actual tool with an argument
/// shape that tool's own `input_schema` requires — not `echo`'s hardcoded
/// `{ message: "hello" }`. `llm`'s `complete` tool requires `prompt`; a page
/// that still sends `message` fails schema validation on the very first
/// click, which is exactly the "scaffold that doesn't work" failure 1.2.7
/// and 1.2.8 already paid for.
#[test]
fn scaffold_with_view_derives_the_kinds_own_argument_shape() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("llmargs");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("llmargs")
        .arg("--kind")
        .arg("llm")
        .arg("--with-view")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(ok, "scaffold failed: {err}");

    let app_js = std::fs::read_to_string(target.join("assets/views/hello/app.js"))
        .expect("read scaffolded app.js");
    // Checked as a quoted JSON key (`"prompt"` / `"message"`) rather than a
    // bare substring: app.js's own error handling legitimately reads
    // `err.message`, so a bare `contains("message")` would false-positive on
    // that unrelated property access rather than on the tool's argument key.
    assert!(
        app_js.contains("\"prompt\""),
        "app.js must send the argument llm's `complete` tool actually requires (`prompt`): {app_js}"
    );
    assert!(
        !app_js.contains("\"message\""),
        "app.js must not carry over echo's `message` argument shape for a kind that doesn't use it: {app_js}"
    );
}

#[test]
fn with_view_is_rejected_for_kind_mcp() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("routery");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("routery")
        .arg("--kind")
        .arg("mcp")
        .arg("--with-view")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(!ok, "mcp artifacts carry no contributions block at all");
    assert!(
        err.contains("--with-view"),
        "the error must name the flag it rejected: {err}"
    );
}
