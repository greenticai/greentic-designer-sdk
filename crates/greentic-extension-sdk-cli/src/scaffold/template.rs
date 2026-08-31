//! Template rendering and file writing.

use std::{collections::HashMap, fs, path::Path};

use include_dir::{Dir, include_dir};

static TEMPLATES_COMMON: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/common");
static TEMPLATES_DESIGN: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/design");
static TEMPLATES_BUNDLE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/bundle");
static TEMPLATES_DEPLOY: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/deploy");
static TEMPLATES_PROVIDER: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/provider");
static TEMPLATES_WASM_COMPONENT: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/templates/wasm-component");
static TEMPLATES_LLM: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/llm");
static TEMPLATES_MCP: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/mcp");
static TEMPLATES_OPENAPI_CONNECTOR: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/templates/openapi-connector");
static TEMPLATES_VIEW_ADDON: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/view-addon");

#[derive(Debug, Clone)]
pub struct TemplateEntry {
    pub src_bytes: &'static [u8],
    /// Destination relative path inside the project (with `.tmpl` stripped and
    /// `gitignore` renamed to `.gitignore`).
    pub dst_rel: String,
}

fn collect(dir: &'static Dir<'static>) -> Vec<TemplateEntry> {
    let mut out = Vec::new();
    collect_rec(dir, &mut out);
    out
}

fn collect_rec(dir: &'static Dir<'static>, out: &mut Vec<TemplateEntry>) {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::File(f) => {
                let rel = f.path().to_string_lossy().to_string();
                let dst = translate_dst(&rel);
                out.push(TemplateEntry {
                    src_bytes: f.contents(),
                    dst_rel: dst,
                });
            }
            include_dir::DirEntry::Dir(d) => collect_rec(d, out),
        }
    }
}

fn translate_dst(rel: &str) -> String {
    let mut dst = rel.trim_end_matches(".tmpl").to_string();
    if dst == "gitignore" {
        dst = ".gitignore".to_string();
    }
    // The template tree can't hold a literal `.claude/` directory — a dotfile
    // dir under the SDK's own `templates/` would be acted on by local tooling
    // (same reason `.gitignore` ships as `gitignore`). Author it under
    // `claude/` and re-dot it to `.claude/` at write time.
    if let Some(rest) = dst.strip_prefix("claude/") {
        dst = format!(".claude/{rest}");
    }
    dst
}

pub fn load_templates_common() -> Vec<TemplateEntry> {
    collect(&TEMPLATES_COMMON)
}

/// The view id the `view-addon` template tree is authored under. Every entry
/// lands beneath `assets/views/<VIEW_TEMPLATE_ID>/` on disk.
const VIEW_TEMPLATE_ID: &str = "hello";

/// Assets for `gtdx new --with-view`, rehomed under `assets/views/<view_id>/`.
///
/// An additive overlay rather than a kind: a view is a contribution, so it
/// layers onto whichever kind the author chose instead of replacing it.
///
/// The rehoming is not cosmetic. `contributions.views[].entry` resolves against
/// `assets/views/<the view's own id>/`, so a template tree left at its authored
/// `hello` path while the describe names another id produces a view whose page
/// is not where the describe says it is — `E_VIEW_ENTRY_MISSING`, on a project
/// the author configured exactly as documented.
pub fn load_templates_view_addon(view_id: &str) -> Vec<TemplateEntry> {
    let authored = format!("assets/views/{VIEW_TEMPLATE_ID}/");
    let wanted = format!("assets/views/{view_id}/");
    collect(&TEMPLATES_VIEW_ADDON)
        .into_iter()
        .map(|mut entry| {
            entry.dst_rel = entry.dst_rel.replacen(&authored, &wanted, 1);
            entry
        })
        .collect()
}

/// Layer `over` on top of `base`, keyed by destination path: an entry in
/// `over` replaces the `base` entry that would land at the same path, and new
/// paths are appended. Order within `base` is preserved so the generated file
/// list stays stable.
fn overlay(base: Vec<TemplateEntry>, over: Vec<TemplateEntry>) -> Vec<TemplateEntry> {
    let mut out = base;
    for entry in over {
        if let Some(slot) = out.iter_mut().find(|e| e.dst_rel == entry.dst_rel) {
            *slot = entry;
        } else {
            out.push(entry);
        }
    }
    out
}

pub fn load_templates_kind(kind: &str) -> Vec<TemplateEntry> {
    match kind {
        "design" => collect(&TEMPLATES_DESIGN),
        "bundle" => collect(&TEMPLATES_BUNDLE),
        "deploy" => collect(&TEMPLATES_DEPLOY),
        "provider" => collect(&TEMPLATES_PROVIDER),
        // `wasm-component` is a `design` extension whose describe additionally
        // declares the OCI component that executes its palette node, so it
        // reuses the design crate wholesale and overrides only the files that
        // genuinely differ. It used to carry its own two-crate workspace
        // (`extension/` + `runtime/`), which duplicated the crate, drifted
        // against the contract, and put the vendored WIT deps outside the
        // crate's target path so nothing it generated ever built.
        "wasm-component" => overlay(
            collect(&TEMPLATES_DESIGN),
            collect(&TEMPLATES_WASM_COMPONENT),
        ),
        "llm" => collect(&TEMPLATES_LLM),
        "mcp" => collect(&TEMPLATES_MCP),
        "openapi-connector" => collect(&TEMPLATES_OPENAPI_CONNECTOR),
        _ => Vec::new(),
    }
}

/// Whether a kind's scaffold contributes at least one tool.
///
/// Derived from the kind's own `describe.json.tmpl` rather than a hand-written
/// list of kinds, which is the shape that has gone stale here before. Every
/// placeholder in a describe template sits inside a JSON string
/// (`"{{runtime_ref_key}}"`), so the template parses as JSON without being
/// rendered first.
///
/// Fails **open**: a template this cannot parse, or a kind with no template at
/// all, reports `true` so the caller offers the option and the authoritative
/// check — which runs against the rendered describe — produces the real error.
/// Reporting `false` would silently hide a legitimate choice instead.
#[must_use]
pub fn kind_contributes_tools(kind: &str) -> bool {
    let entries = load_templates_kind(kind);
    let Some(describe) = entries
        .iter()
        .find(|e| e.dst_rel.ends_with("describe.json"))
    else {
        return true;
    };
    let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(describe.src_bytes) else {
        return true;
    };
    parsed
        .pointer("/contributions/tools")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|tools| !tools.is_empty())
}

pub struct Context {
    values: HashMap<&'static str, String>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: &'static str, value: impl Into<String>) -> &mut Self {
        self.values.insert(key, value.into());
        self
    }

    pub fn render(&self, template: &str) -> anyhow::Result<String> {
        let mut out = template.to_string();
        let mut remaining_passes = 4;
        while remaining_passes > 0 {
            let before = out.clone();
            for (key, value) in &self.values {
                let token = format!("{{{{{key}}}}}");
                out = out.replace(&token, value);
            }
            if out == before {
                break;
            }
            remaining_passes -= 1;
        }
        if let Some(pos) = out.find("{{") {
            let end = out[pos..].find("}}").map_or(out.len(), |e| pos + e + 2);
            anyhow::bail!("unsubstituted placeholder: {}", &out[pos..end]);
        }
        Ok(out)
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

pub fn write_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

#[allow(dead_code)] // test-only helper; kept for API ergonomics
pub fn render_and_write(ctx: &Context, template: &str, path: &Path) -> anyhow::Result<()> {
    let rendered = ctx.render(template)?;
    write_file(path, rendered.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `--kind` value, derived from the clap enum rather than hand-listed,
    /// so a kind added to `Kind` cannot silently skip these guards.
    fn all_kind_strs() -> Vec<&'static str> {
        use clap::ValueEnum as _;
        crate::scaffold::Kind::value_variants()
            .iter()
            .map(|k| k.as_str())
            .collect()
    }

    #[test]
    fn render_substitutes_placeholder() {
        let mut ctx = Context::new();
        ctx.set("name", "demo");
        let out = ctx.render("hello {{name}}!").unwrap();
        assert_eq!(out, "hello demo!");
    }

    #[test]
    fn render_multiple_placeholders() {
        let mut ctx = Context::new();
        ctx.set("name", "demo").set("version", "0.1.0");
        let out = ctx.render("{{name}}@{{version}}").unwrap();
        assert_eq!(out, "demo@0.1.0");
    }

    #[test]
    fn render_unsubstituted_placeholder_errors() {
        let ctx = Context::new();
        let err = ctx.render("hello {{missing}}").unwrap_err();
        assert!(err.to_string().contains("{{missing}}"));
    }

    #[test]
    fn render_literal_text_passthrough() {
        let ctx = Context::new();
        let out = ctx.render("plain text no braces").unwrap();
        assert_eq!(out, "plain text no braces");
    }

    #[test]
    fn write_file_creates_parent_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("a/b/c/file.txt");
        write_file(&dst, b"hello").unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
    }

    #[test]
    fn render_and_write_substitutes_before_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("out.txt");
        let mut ctx = Context::new();
        ctx.set("who", "world");
        render_and_write(&ctx, "hello {{who}}", &dst).unwrap();
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "hello world");
    }

    #[test]
    fn load_common_returns_gitignore_template() {
        let entries = load_templates_common();
        assert!(
            entries
                .iter()
                .any(|e| e.dst_rel == "gitignore.tmpl" || e.dst_rel == ".gitignore")
        );
    }

    /// Every scaffold ships agent-onboarding docs so AI coding tools (Claude
    /// Code, Codex, …) pick up the build/publish workflow and the
    /// placeholder-vs-generated distinctions without re-deriving them. AGENTS.md
    /// is the universal source of truth; CLAUDE.md is a thin pointer to it.
    /// Both live in `common/`, so they apply to every kind.
    #[test]
    fn load_common_returns_agent_onboarding_docs() {
        let entries = load_templates_common();
        for expected in ["AGENTS.md", "CLAUDE.md"] {
            assert!(
                entries.iter().any(|e| e.dst_rel == expected),
                "common templates missing {expected}: {:?}",
                entries.iter().map(|e| &e.dst_rel).collect::<Vec<_>>(),
            );
        }
        // CLAUDE.md must point readers at AGENTS.md so the two never drift.
        let claude = entries
            .iter()
            .find(|e| e.dst_rel == "CLAUDE.md")
            .expect("CLAUDE.md present");
        let body = std::str::from_utf8(claude.src_bytes).expect("utf8");
        assert!(
            body.contains("AGENTS.md"),
            "CLAUDE.md must reference AGENTS.md:\n{body}",
        );
    }

    /// Every scaffold ships Claude Code project config so AI coding tools run the
    /// build/check commands without per-command permission prompts (`settings.json`)
    /// and get a one-shot pre-publish gate (`/check`). Both are authored under
    /// `claude/` and re-dotted to `.claude/` by [`translate_dst`].
    #[test]
    fn load_common_returns_dotclaude_config() {
        let entries = load_templates_common();
        for expected in [".claude/settings.json", ".claude/commands/check.md"] {
            assert!(
                entries.iter().any(|e| e.dst_rel == expected),
                "common templates missing {expected}: {:?}",
                entries.iter().map(|e| &e.dst_rel).collect::<Vec<_>>(),
            );
        }
        // settings.json must parse as JSON and pre-approve the core build commands.
        let settings = entries
            .iter()
            .find(|e| e.dst_rel == ".claude/settings.json")
            .expect(".claude/settings.json present");
        let body = std::str::from_utf8(settings.src_bytes).expect("utf8");
        let parsed: serde_json::Value = serde_json::from_str(body).expect("settings.json parses");
        let allow = parsed
            .get("permissions")
            .and_then(|p| p.get("allow"))
            .and_then(|a| a.as_array())
            .expect("permissions.allow is an array");
        assert!(
            allow.iter().any(|v| v.as_str() == Some("Bash(gtdx:*)")),
            "settings.json must pre-approve gtdx commands:\n{body}",
        );
    }

    #[test]
    fn load_kind_design_returns_cargo_toml() {
        let entries = load_templates_kind("design");
        assert!(entries.iter().any(|e| e.dst_rel == "Cargo.toml"));
        assert!(entries.iter().any(|e| e.dst_rel == "describe.json"));
        assert!(entries.iter().any(|e| e.dst_rel == "src/lib.rs"));
    }

    /// Audit P1 (E.3.a): every kind must scaffold a rust-toolchain.toml
    /// pinned to the same workspace toolchain. Without this the user's
    /// `cargo build` picks up whatever's in their PATH and silently
    /// produces wasm with the wrong feature set.
    #[test]
    fn every_kind_template_ships_rust_toolchain_pinned_to_1_95_0() {
        for kind in [
            "design",
            "bundle",
            "deploy",
            "provider",
            "wasm-component",
            "llm",
            "mcp",
        ] {
            let entries = load_templates_kind(kind);
            let toolchain = entries
                .iter()
                .find(|e| e.dst_rel == "rust-toolchain.toml")
                .unwrap_or_else(|| panic!("kind {kind} missing rust-toolchain.toml template"));
            let content = std::str::from_utf8(toolchain.src_bytes).expect("utf8");
            assert!(
                content.contains("channel = \"1.95.0\""),
                "kind {kind} toolchain template does not pin 1.95.0:\n{content}",
            );
        }
    }

    /// `build.sh` must locate the built `.wasm` by globbing
    /// `target/wasm32-wasip*/release/` and fail loudly if nothing was
    /// produced, rather than `cd`-ing straight into
    /// `target/wasm32-wasip2/release`. `cargo component build --release`
    /// writes under a `wasm32-wasip1`-named directory even when the
    /// toolchain targets wasip2, so the hard-coded path made every freshly
    /// scaffolded project's own documented build step fail with "No such
    /// file or directory" (ported from PR #156 / SDK 1.2.14).
    #[test]
    fn common_build_sh_locates_wasm_by_glob_instead_of_hardcoded_path() {
        let entries = load_templates_common();
        let build_sh = entries
            .iter()
            .find(|e| e.dst_rel == "build.sh")
            .expect("build.sh present");
        let content = std::str::from_utf8(build_sh.src_bytes).expect("utf8");
        assert!(
            !content.contains("cd target/wasm32-wasip2/release"),
            "build.sh must not hard-code target/wasm32-wasip2/release, cargo \
             component build can write to wasip1 instead:\n{content}",
        );
        assert!(
            content.contains("target/wasm32-wasip*/release/"),
            "build.sh must glob target/wasm32-wasip*/release/ to find the \
             built component regardless of which wasip cargo-component used:\n{content}",
        );
        assert!(
            content.contains("exit 1"),
            "build.sh must fail loudly when the build produced no .wasm:\n{content}",
        );
    }

    /// `ci/local_check.sh` must run `cargo component bindings` before
    /// `cargo fmt`, so `src/bindings.rs` exists before anything tries to
    /// resolve `mod bindings;`. Without it, fmt/clippy/test all fail to
    /// compile on a freshly scaffolded, unbuilt project (ported from PR
    /// #156 / SDK 1.2.14).
    #[test]
    fn common_local_check_sh_generates_bindings_before_fmt() {
        let entries = load_templates_common();
        let local_check = entries
            .iter()
            .find(|e| e.dst_rel == "ci/local_check.sh")
            .expect("ci/local_check.sh present");
        let content = std::str::from_utf8(local_check.src_bytes).expect("utf8");
        let bindings_pos = content.find("cargo component bindings").unwrap_or_else(|| {
            panic!("ci/local_check.sh must run `cargo component bindings` before fmt:\n{content}")
        });
        let fmt_pos = content
            .find("cargo fmt --all -- --check")
            .expect("ci/local_check.sh must run cargo fmt --all -- --check");
        assert!(
            bindings_pos < fmt_pos,
            "ci/local_check.sh must generate bindings before running cargo fmt:\n{content}",
        );
    }

    /// Every kind whose `src/lib.rs` declares `mod bindings;` for a
    /// wit-bindgen-generated `src/bindings.rs` must mark that declaration
    /// `#[rustfmt::skip]`. The generated file doesn't exist before the first
    /// build (so `cargo fmt --all -- --check` can't even resolve the module)
    /// and is never rustfmt-clean once it does exist — failing the check on
    /// a freshly scaffolded, unbuilt project either way (ported from PR
    /// #156 / SDK 1.2.14). `mcp` is exempt: it generates bindings inline via
    /// `wit_bindgen::generate!` and has no `mod bindings;` declaration to
    /// skip.
    #[test]
    fn every_kind_skips_rustfmt_on_generated_bindings_module() {
        for kind in ["design", "bundle", "deploy", "provider", "llm"] {
            let entries = load_templates_kind(kind);
            let lib_rs = entries
                .iter()
                .find(|e| e.dst_rel == "src/lib.rs")
                .unwrap_or_else(|| panic!("kind {kind} missing src/lib.rs template"));
            let content = std::str::from_utf8(lib_rs.src_bytes).expect("utf8");
            let bindings_pos = content.find("mod bindings;").unwrap_or_else(|| {
                panic!("kind {kind} src/lib.rs has no `mod bindings;` declaration:\n{content}")
            });
            let preceding = &content[..bindings_pos];
            let last_line = preceding
                .trim_end_matches('\n')
                .lines()
                .last()
                .unwrap_or("");
            assert_eq!(
                last_line.trim(),
                "#[rustfmt::skip]",
                "kind {kind} `mod bindings;` must be immediately preceded by \
                 #[rustfmt::skip] so cargo fmt --all -- --check passes before \
                 the generated file exists:\n{content}",
            );
        }
    }

    /// Audit P1 (E.3.b): extension kinds that use cargo-component's generated
    /// bindings must scaffold `wit-bindgen-rt = "0.41"` (the current pinned
    /// version for those kinds). 0.35 emits older intrinsics that newer
    /// cargo-component rejects.
    ///
    /// The `mcp` kind is exempt: it uses inline `wit_bindgen::generate!` via
    /// `wit-bindgen` (macros feature) so the crate compiles on the host as
    /// well as wasm32. It must not use `wit-bindgen-rt` directly.
    #[test]
    fn every_kind_template_ships_wit_bindgen_rt_0_41() {
        for kind in ["design", "bundle", "deploy", "provider", "llm"] {
            let entries = load_templates_kind(kind);
            let cargo_toml = entries
                .iter()
                .find(|e| e.dst_rel == "Cargo.toml")
                .unwrap_or_else(|| panic!("kind {kind} missing Cargo.toml template"));
            let content = std::str::from_utf8(cargo_toml.src_bytes).expect("utf8");
            assert!(
                content.contains("wit-bindgen-rt = { version = \"0.41\""),
                "kind {kind} Cargo.toml does not pin wit-bindgen-rt 0.41:\n{content}",
            );
        }
    }

    /// The `mcp` kind uses inline `wit_bindgen::generate!` (host-compatible
    /// bindings) and must depend on `wit-bindgen` with the `macros` feature,
    /// not the older `wit-bindgen-rt` shim.
    #[test]
    fn mcp_kind_template_ships_wit_bindgen_macros() {
        let entries = load_templates_kind("mcp");
        let cargo_toml = entries
            .iter()
            .find(|e| e.dst_rel == "Cargo.toml")
            .unwrap_or_else(|| panic!("mcp kind missing Cargo.toml template"));
        let content = std::str::from_utf8(cargo_toml.src_bytes).expect("utf8");
        assert!(
            content.contains("wit-bindgen") && content.contains("macros"),
            "mcp Cargo.toml must depend on wit-bindgen with macros feature:\n{content}",
        );
        assert!(
            !content.contains("wit-bindgen-rt"),
            "mcp Cargo.toml must NOT use wit-bindgen-rt (use wit-bindgen + macros instead):\n{content}",
        );
    }

    /// The `wasm-component` scaffold is a standalone cargo-component project
    /// with no parent workspace, so its Cargo.toml must use concrete
    /// `[package]` fields, not `edition.workspace = true` / `*.workspace`
    /// inherits — which would make `cargo build` fail on a fresh scaffold
    /// (audit cycle-2 N12).
    #[test]
    fn wasm_component_cargo_toml_has_no_workspace_inherits() {
        let entries = load_templates_kind("wasm-component");
        let cargo_toml = entries
            .iter()
            .find(|e| e.dst_rel.ends_with("Cargo.toml"))
            .expect("wasm-component must ship a Cargo.toml template");
        let content = std::str::from_utf8(cargo_toml.src_bytes).expect("utf8");
        assert!(
            !content.contains(".workspace = true") && !content.contains(".workspace=true"),
            "wasm-component Cargo.toml must not inherit from a (non-existent) workspace:\n{content}",
        );
        assert!(
            content.contains("edition = \"2024\""),
            "wasm-component Cargo.toml must pin edition concretely:\n{content}",
        );
    }

    /// Every kind's `describe.json` template must emit v2 shape — after
    /// the v1->v2 ecosystem migration (PR-series ending in greentic-biz/
    /// greentic-designer-extensions#58), scaffolded extensions that still
    /// emit `apiVersion: greentic.ai/v1` won't round-trip through the
    /// 1.2.x runtime without manual editing. This guard catches any
    /// future template that regresses to v1 shape.
    #[test]
    fn every_kind_describe_template_is_v2() {
        for kind in [
            "design",
            "bundle",
            "deploy",
            "provider",
            "wasm-component",
            "llm",
            "mcp",
        ] {
            let entries = load_templates_kind(kind);
            let describe = entries
                .iter()
                .find(|e| e.dst_rel == "describe.json")
                .unwrap_or_else(|| panic!("kind {kind} missing describe.json template"));
            let content = std::str::from_utf8(describe.src_bytes).expect("utf8");
            assert!(
                content.contains("\"apiVersion\": \"greentic.ai/v2\""),
                "kind {kind} describe.json template not v2:\n{content}",
            );
            assert!(
                content.contains("\"compat\":"),
                "kind {kind} describe.json missing `compat` block:\n{content}",
            );
            assert!(
                content.contains("\"components\":"),
                "kind {kind} describe.json missing `runtime.components` map:\n{content}",
            );
            assert!(
                !content.contains("\"component\": \"extension.wasm\""),
                "kind {kind} describe.json still has v1 singular `runtime.component`:\n{content}",
            );
        }
    }

    /// The page must land under the id the describe names, or the scaffold
    /// ships a view whose entry file is not where its own describe points.
    #[test]
    fn view_assets_are_rehomed_under_the_chosen_id() {
        let entries = load_templates_view_addon("usage");
        assert!(!entries.is_empty(), "the view overlay must ship files");
        for entry in &entries {
            assert!(
                entry.dst_rel.starts_with("assets/views/usage/"),
                "{} was not rehomed",
                entry.dst_rel
            );
        }
        assert!(
            entries
                .iter()
                .any(|e| e.dst_rel == "assets/views/usage/index.html"),
            "the entry HTML must be present under the chosen id"
        );
    }

    /// The default id is the one the tree is authored under, so the common
    /// case rewrites to exactly what it already was.
    #[test]
    fn the_default_view_id_leaves_the_authored_paths_alone() {
        let default = load_templates_view_addon(VIEW_TEMPLATE_ID);
        assert!(
            default.iter().all(|e| e
                .dst_rel
                .starts_with(&format!("assets/views/{VIEW_TEMPLATE_ID}/"))),
            "authored paths changed under the default id"
        );
    }

    /// Every kind's describe template must parse as JSON without being
    /// rendered first — that is what lets `kind_contributes_tools` and the
    /// wizard's per-kind gating read the template instead of carrying a
    /// hand-written list of kinds.
    ///
    /// It holds because every placeholder sits inside a JSON string. A
    /// template that interpolated one bare (`"memoryLimitMB": {{mb}}`) would
    /// break the derivation, and this test is where that shows up.
    #[test]
    fn every_kind_describe_template_parses_as_json() {
        for kind in all_kind_strs() {
            let entries = load_templates_kind(kind);
            let describe = entries
                .iter()
                .find(|e| e.dst_rel == "describe.json")
                .unwrap_or_else(|| panic!("kind {kind} missing describe.json template"));
            serde_json::from_slice::<serde_json::Value>(describe.src_bytes)
                .unwrap_or_else(|e| panic!("kind {kind} describe.json.tmpl is not JSON: {e}"));
        }
    }

    /// `memoryLimitMB` defaults to 64 when omitted, so a template that never
    /// writes it produces extensions whose authors have no way to learn the
    /// field exists. Declaring the default explicitly is what makes it
    /// discoverable — and `mcp` already did, which is how the gap in the other
    /// eight went unnoticed.
    #[test]
    fn every_kind_declares_its_memory_limit_explicitly() {
        for kind in all_kind_strs() {
            let entries = load_templates_kind(kind);
            let describe = entries
                .iter()
                .find(|e| e.dst_rel == "describe.json")
                .unwrap_or_else(|| panic!("kind {kind} missing describe.json template"));
            let parsed: serde_json::Value =
                serde_json::from_slice(describe.src_bytes).expect("template is JSON");
            let mb = parsed
                .pointer("/runtime/memoryLimitMB")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_else(|| panic!("kind {kind} does not declare runtime.memoryLimitMB"));
            assert!(
                (1..=1024).contains(&mb),
                "kind {kind} declares memoryLimitMB {mb}, outside the contract bound 1..=1024"
            );
        }
    }

    /// The tool-surface flag is only offered for kinds that contribute tools,
    /// and that answer is read off the template rather than hand-listed.
    #[test]
    fn tool_contribution_is_read_off_each_kind_template() {
        for kind in all_kind_strs() {
            let entries = load_templates_kind(kind);
            let describe = entries
                .iter()
                .find(|e| e.dst_rel == "describe.json")
                .expect("describe template");
            let parsed: serde_json::Value =
                serde_json::from_slice(describe.src_bytes).expect("template is JSON");
            let expected = parsed
                .pointer("/contributions/tools")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|tools| !tools.is_empty());
            assert_eq!(
                kind_contributes_tools(kind),
                expected,
                "kind {kind}: derivation disagrees with its own template"
            );
        }
    }

    /// Fails open, so an unparseable or missing template offers the choice and
    /// lets the authoritative post-render check produce the real error.
    #[test]
    fn tool_contribution_fails_open_for_an_unknown_kind() {
        assert!(kind_contributes_tools("no-such-kind"));
    }

    /// E.4.b: the `llm` template tree resolves through `load_templates_kind`
    /// and ships the canonical 5-file design-extension skeleton.
    #[test]
    fn load_kind_llm_returns_full_template_set() {
        let entries = load_templates_kind("llm");
        let names: Vec<&str> = entries.iter().map(|e| e.dst_rel.as_str()).collect();
        assert!(
            names.contains(&"Cargo.toml"),
            "missing Cargo.toml: {names:?}"
        );
        assert!(
            names.contains(&"describe.json"),
            "missing describe.json: {names:?}"
        );
        assert!(
            names.contains(&"src/lib.rs"),
            "missing src/lib.rs: {names:?}"
        );
        assert!(
            names.contains(&"wit/world.wit"),
            "missing wit/world.wit: {names:?}"
        );
        assert!(
            names.contains(&"rust-toolchain.toml"),
            "missing rust-toolchain.toml: {names:?}"
        );
    }

    /// The `mcp` template tree resolves through `load_templates_kind` and ships
    /// the single-file `wasix:mcp/router` skeleton plus the bundled `wasix-mcp`
    /// WIT dep so `cargo component build` resolves the exported router
    /// interface.
    #[test]
    fn load_kind_mcp_returns_full_template_set() {
        let entries = load_templates_kind("mcp");
        let names: Vec<&str> = entries.iter().map(|e| e.dst_rel.as_str()).collect();
        for expected in [
            "Cargo.toml",
            "describe.json",
            "src/lib.rs",
            "wit/world.wit",
            "wit/deps/wasix-mcp/package.wit",
            "rust-toolchain.toml",
        ] {
            assert!(names.contains(&expected), "missing {expected}: {names:?}");
        }
    }
}
