//! Extension icon authoring: validate an icon file, copy it into `assets/`,
//! and set `metadata.icon` in describe.json. Shared by `gtdx new` and
//! `gtdx publish`.

use std::path::Path;

use anyhow::{Context, Result, bail};

/// Max icon size (1 MiB) — matches the store-server icon cap.
// Not yet called from `main` in this commit: `gtdx new`/`gtdx publish` wire
// this in next (see plan Tasks 2-3).
#[allow(dead_code)]
pub const MAX_ICON_BYTES: u64 = 1024 * 1024;

/// Icon file extensions the store + designer render.
#[allow(dead_code)]
const SUPPORTED_EXTS: &[&str] = &["svg", "png", "jpg", "jpeg", "webp"];

/// Validate `icon_src` (type + size), copy it to `<project_dir>/assets/icon.<ext>`,
/// and set `metadata.icon` in `<project_dir>/describe.json` to that pack-relative
/// path. Idempotent (an existing `metadata.icon` is overwritten). Returns the
/// relative icon path, e.g. `"assets/icon.svg"`.
#[allow(dead_code)]
pub fn apply_icon(project_dir: &Path, icon_src: &Path) -> Result<String> {
    let ext = icon_src
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| SUPPORTED_EXTS.contains(&e.as_str()))
        .with_context(|| {
            format!(
                "unsupported icon type for {} (supported: svg, png, jpg, jpeg, webp)",
                icon_src.display()
            )
        })?;

    let bytes =
        std::fs::read(icon_src).with_context(|| format!("read icon {}", icon_src.display()))?;
    let len = bytes.len() as u64;
    if len > MAX_ICON_BYTES {
        bail!(
            "icon {} is {len} bytes; max is {MAX_ICON_BYTES} (1 MiB)",
            icon_src.display()
        );
    }

    let rel = format!("assets/icon.{ext}");
    let dst = project_dir.join(&rel);
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(&dst, &bytes).with_context(|| format!("write {}", dst.display()))?;

    let describe_path = project_dir.join("describe.json");
    let raw = std::fs::read(&describe_path)
        .with_context(|| format!("read {}", describe_path.display()))?;
    let mut describe: serde_json::Value = serde_json::from_slice(&raw)
        .with_context(|| format!("parse {}", describe_path.display()))?;
    let meta = describe
        .get_mut("metadata")
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("{} has no metadata object", describe_path.display()))?;
    meta.insert("icon".to_string(), serde_json::Value::String(rel.clone()));

    let mut out = serde_json::to_vec_pretty(&describe).context("serialize describe.json")?;
    out.push(b'\n');
    std::fs::write(&describe_path, out)
        .with_context(|| format!("write {}", describe_path.display()))?;

    Ok(rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";

    /// Write a minimal valid v2 describe.json into `dir`.
    fn seed_describe(dir: &std::path::Path) {
        let describe = r#"{"apiVersion":"greentic.ai/v2","kind":"DesignExtension","metadata":{"id":"greentic.demo","name":"Demo","version":"0.1.0","keywords":[]}}"#;
        fs::write(dir.join("describe.json"), describe).unwrap();
    }

    #[test]
    fn applies_svg_and_patches_describe() {
        let proj = tempfile::tempdir().unwrap();
        seed_describe(proj.path());
        let src = proj.path().join("logo.svg");
        fs::write(&src, SVG).unwrap();

        let rel = apply_icon(proj.path(), &src).unwrap();

        assert_eq!(rel, "assets/icon.svg");
        assert_eq!(fs::read(proj.path().join("assets/icon.svg")).unwrap(), SVG);
        let d: serde_json::Value =
            serde_json::from_slice(&fs::read(proj.path().join("describe.json")).unwrap()).unwrap();
        assert_eq!(d["metadata"]["icon"], "assets/icon.svg");
    }

    #[test]
    fn preserves_png_extension() {
        let proj = tempfile::tempdir().unwrap();
        seed_describe(proj.path());
        let src = proj.path().join("logo.png");
        fs::write(&src, b"\x89PNG\r\n").unwrap();

        let rel = apply_icon(proj.path(), &src).unwrap();

        assert_eq!(rel, "assets/icon.png");
        assert!(proj.path().join("assets/icon.png").exists());
    }

    #[test]
    fn rejects_unsupported_extension() {
        let proj = tempfile::tempdir().unwrap();
        seed_describe(proj.path());
        let src = proj.path().join("logo.gif");
        fs::write(&src, b"GIF89a").unwrap();
        assert!(apply_icon(proj.path(), &src).is_err());
    }

    #[test]
    fn rejects_missing_file() {
        let proj = tempfile::tempdir().unwrap();
        seed_describe(proj.path());
        assert!(apply_icon(proj.path(), &proj.path().join("nope.svg")).is_err());
    }

    #[test]
    fn rejects_oversize() {
        let proj = tempfile::tempdir().unwrap();
        seed_describe(proj.path());
        let src = proj.path().join("big.svg");
        fs::write(
            &src,
            vec![b'a'; usize::try_from(MAX_ICON_BYTES + 1).unwrap()],
        )
        .unwrap();
        assert!(apply_icon(proj.path(), &src).is_err());
    }
}
