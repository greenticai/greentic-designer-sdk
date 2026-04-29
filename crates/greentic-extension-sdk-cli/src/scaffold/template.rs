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
    dst
}

pub fn load_templates_common() -> Vec<TemplateEntry> {
    collect(&TEMPLATES_COMMON)
}

pub fn load_templates_kind(kind: &str) -> Vec<TemplateEntry> {
    match kind {
        "design" => collect(&TEMPLATES_DESIGN),
        "bundle" => collect(&TEMPLATES_BUNDLE),
        "deploy" => collect(&TEMPLATES_DEPLOY),
        "provider" => collect(&TEMPLATES_PROVIDER),
        "wasm-component" => collect(&TEMPLATES_WASM_COMPONENT),
        _ => Vec::new(),
    }
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

    #[test]
    fn load_kind_design_returns_cargo_toml() {
        let entries = load_templates_kind("design");
        assert!(entries.iter().any(|e| e.dst_rel == "Cargo.toml"));
        assert!(entries.iter().any(|e| e.dst_rel == "describe.json"));
        assert!(entries.iter().any(|e| e.dst_rel == "src/lib.rs"));
    }
}
