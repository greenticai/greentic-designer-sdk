//! `.gtxpack` builder: stages describe + wasm + assets and hands off to the
//! shared `greentic-extension-sdk-contract::pack_writer` for deterministic ZIP emission.

use std::path::{Path, PathBuf};

use greentic_extension_sdk_contract::pack_writer::{PackEntry, build_gtxpack, sha256_hex};
use walkdir::WalkDir;

/// Summary of a packed `.gtxpack`.
#[derive(Debug, Clone)]
pub struct PackInfo {
    pub pack_path: PathBuf,
    pub pack_name: String,
    pub size: u64,
    pub sha256: String,
    pub ext_name: String,
    pub ext_version: String,
    #[allow(dead_code)] // Reserved for richer InstallOk events in Phase 2.
    pub ext_kind: String,
}

/// Build a `.gtxpack` at `output_pack` from `project_dir` + the already-built
/// `wasm_path`. The ZIP contains `describe.json`, the wasm renamed to
/// `extension.wasm` (matches `runtime.component` default), and any optional
/// asset dirs that exist (`i18n/`, `schemas/`, `prompts/`).
///
/// For Provider extensions, if `describe.runtime.gtpack` is set (non-null),
/// the referenced file is read, sha256-verified, and embedded in the archive.
pub fn build_pack(
    project_dir: &Path,
    wasm_path: &Path,
    output_pack: &Path,
) -> anyhow::Result<PackInfo> {
    let describe_path = project_dir.join("describe.json");
    let describe_bytes =
        std::fs::read(&describe_path).map_err(|e| anyhow::anyhow!("read describe.json: {e}"))?;
    let describe: serde_json::Value = serde_json::from_slice(&describe_bytes)
        .map_err(|e| anyhow::anyhow!("parse describe.json: {e}"))?;
    let ext_name = describe["metadata"]["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("describe.metadata.name missing"))?
        .to_string();
    let ext_version = describe["metadata"]["version"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("describe.metadata.version missing"))?
        .to_string();
    let ext_kind = describe["kind"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("describe.kind missing"))?
        .to_string();

    let mut entries = vec![
        PackEntry::file("describe.json", describe_bytes),
        PackEntry::file("extension.wasm", std::fs::read(wasm_path)?),
    ];

    // Provider extensions embed a runtime .gtpack alongside the metadata WASM.
    // describe.runtime.gtpack is required when kind=ProviderExtension (enforced
    // at deserialize time) and points at a project-relative file path with a
    // declared sha256 that the install path verifies.
    //
    // Value::get returns Some(Value::Null) for present-but-null fields, so
    // the .filter(|v| !v.is_null()) correctly skips Design/Bundle/Deploy
    // extensions that have no gtpack field or have it explicitly set to null.
    if let Some(gtpack) = describe["runtime"].get("gtpack").filter(|v| !v.is_null()) {
        let file_rel = gtpack["file"].as_str().ok_or_else(|| {
            anyhow::anyhow!("describe.runtime.gtpack.file missing or not a string")
        })?;
        let expected_sha = gtpack["sha256"].as_str().ok_or_else(|| {
            anyhow::anyhow!("describe.runtime.gtpack.sha256 missing or not a string")
        })?;
        let abs = project_dir.join(file_rel);
        if !abs.exists() {
            anyhow::bail!(
                "describe.runtime.gtpack.file = {:?} but file not found at {}.\n\
                 Provider extensions must stage their runtime .gtpack into the project before publish.\n\
                 For pilot/dev, ship a placeholder file at the declared path with sha256 matching describe.json.",
                file_rel,
                abs.display()
            );
        }
        let bytes = std::fs::read(&abs)
            .map_err(|e| anyhow::anyhow!("read runtime gtpack at {}: {e}", abs.display()))?;
        let actual_sha = sha256_hex(&bytes);
        if actual_sha != expected_sha {
            anyhow::bail!(
                "describe.runtime.gtpack.sha256 mismatch for {file_rel}:\n\
                 declared: {expected_sha}\n\
                 actual:   {actual_sha}\n\
                 Either rebuild the runtime + update describe.json, or update describe.json to match the staged file."
            );
        }
        entries.push(PackEntry::file(file_rel.to_string(), bytes));
    }

    for asset_dir in ["i18n", "schemas", "prompts"] {
        let src = project_dir.join(asset_dir);
        if !src.is_dir() {
            continue;
        }
        let mut paths: Vec<PathBuf> = WalkDir::new(&src)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();
        paths.sort();
        for abs in paths {
            let rel = abs
                .strip_prefix(project_dir)
                .expect("asset under project")
                .to_string_lossy()
                .replace('\\', "/");
            entries.push(PackEntry::file(rel, std::fs::read(&abs)?));
        }
    }

    if let Some(parent) = output_pack.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let zip_bytes = build_gtxpack(entries).map_err(|e| anyhow::anyhow!("build_gtxpack: {e}"))?;
    std::fs::write(output_pack, &zip_bytes)?;

    let size = u64::try_from(zip_bytes.len()).unwrap_or(u64::MAX);
    let pack_name = output_pack
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("pack.gtxpack")
        .to_string();
    let sha256 = sha256_hex(&zip_bytes);

    Ok(PackInfo {
        pack_path: output_pack.to_path_buf(),
        pack_name,
        size,
        sha256,
        ext_name,
        ext_version,
        ext_kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn make_project(root: &Path) -> PathBuf {
        let desc = br#"{
  "apiVersion": "greentic.ai/v1",
  "kind": "DesignExtension",
  "metadata": {"id": "com.example.demo", "name": "demo", "version": "0.1.0", "summary": "x", "author": {"name": "a"}, "license": "Apache-2.0"},
  "engine": {"greenticDesigner": "^0.1.0", "extRuntime": "^0.1.0"},
  "capabilities": {"offered": [], "required": []},
  "runtime": {"component": "extension.wasm", "permissions": {"network": [], "secrets": [], "callExtensionKinds": []}},
  "contributions": {}
}"#;
        std::fs::write(root.join("describe.json"), desc).unwrap();
        let wasm_dir = root.join("target/wasm32-wasip2/debug");
        std::fs::create_dir_all(&wasm_dir).unwrap();
        let wasm = wasm_dir.join("demo.wasm");
        std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();
        wasm
    }

    #[test]
    fn build_pack_produces_zip_with_describe_and_wasm() {
        let tmp = tempfile::tempdir().unwrap();
        let wasm = make_project(tmp.path());
        let out = tmp.path().join("dist/demo-0.1.0.gtxpack");
        let info = build_pack(tmp.path(), &wasm, &out).unwrap();
        assert_eq!(info.ext_name, "demo");
        assert_eq!(info.ext_version, "0.1.0");
        assert_eq!(info.ext_kind, "DesignExtension");
        assert!(info.size > 0);

        let file = File::open(&out).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<_> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "describe.json"));
        assert!(names.iter().any(|n| n == "extension.wasm"));
    }

    #[test]
    fn build_pack_is_deterministic_across_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let wasm = make_project(tmp.path());
        let out1 = tmp.path().join("a.gtxpack");
        let out2 = tmp.path().join("b.gtxpack");
        let a = build_pack(tmp.path(), &wasm, &out1).unwrap();
        let b = build_pack(tmp.path(), &wasm, &out2).unwrap();
        assert_eq!(a.sha256, b.sha256);
    }

    #[test]
    fn build_pack_includes_assets_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let wasm = make_project(tmp.path());
        std::fs::create_dir_all(tmp.path().join("i18n")).unwrap();
        std::fs::write(tmp.path().join("i18n/en.json"), br#"{"hello":"world"}"#).unwrap();
        let out = tmp.path().join("demo.gtxpack");
        build_pack(tmp.path(), &wasm, &out).unwrap();
        let file = File::open(&out).unwrap();
        let zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<_> = zip.file_names().map(str::to_string).collect();
        assert!(names.iter().any(|n| n == "i18n/en.json"));
    }

    #[test]
    fn build_pack_errors_if_describe_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let wasm_dir = tmp.path().join("target/wasm32-wasip2/debug");
        std::fs::create_dir_all(&wasm_dir).unwrap();
        std::fs::write(wasm_dir.join("x.wasm"), b"\0asm").unwrap();
        let out = tmp.path().join("out.gtxpack");
        let err = build_pack(tmp.path(), &wasm_dir.join("x.wasm"), &out).unwrap_err();
        assert!(err.to_string().contains("describe.json"));
    }

    // ── Provider extension tests ─────────────────────────────────────────────

    /// Write a minimal provider describe.json with the given runtime.gtpack value.
    fn write_provider_describe(root: &Path, gtpack_field: &serde_json::Value) {
        let desc = serde_json::json!({
            "apiVersion": "greentic.ai/v1",
            "kind": "ProviderExtension",
            "metadata": {
                "id": "com.example.provider",
                "name": "provider",
                "version": "0.1.0",
                "summary": "test provider",
                "author": {"name": "tester"},
                "license": "Apache-2.0"
            },
            "engine": {"greenticDesigner": "^0.1.0", "extRuntime": "^0.1.0"},
            "capabilities": {"offered": [], "required": []},
            "runtime": {
                "component": "extension.wasm",
                "gtpack": gtpack_field,
                "permissions": {"network": [], "secrets": [], "callExtensionKinds": []}
            },
            "contributions": {}
        });
        std::fs::write(root.join("describe.json"), desc.to_string()).unwrap();
    }

    /// Create a complete provider project with a valid `runtime/provider.gtpack`.
    /// Returns (`wasm_path`, `gtpack_bytes`, `sha256_hex`).
    fn make_provider_project(root: &Path) -> (PathBuf, Vec<u8>, String) {
        let gtpack_bytes = b"fake-gtpack-content-for-testing".to_vec();
        let sha = sha256_hex(&gtpack_bytes);

        std::fs::create_dir_all(root.join("runtime")).unwrap();
        std::fs::write(root.join("runtime/provider.gtpack"), &gtpack_bytes).unwrap();

        write_provider_describe(
            root,
            &serde_json::json!({
                "file": "runtime/provider.gtpack",
                "sha256": sha
            }),
        );

        let wasm_dir = root.join("target/wasm32-wasip2/debug");
        std::fs::create_dir_all(&wasm_dir).unwrap();
        let wasm = wasm_dir.join("provider.wasm");
        std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();

        (wasm, gtpack_bytes, sha)
    }

    #[test]
    fn provider_pack_includes_runtime_gtpack() {
        let tmp = tempfile::tempdir().unwrap();
        let (wasm, gtpack_bytes, _sha) = make_provider_project(tmp.path());
        let out = tmp.path().join("dist/provider-0.1.0.gtxpack");

        let info = build_pack(tmp.path(), &wasm, &out).unwrap();
        assert_eq!(info.ext_name, "provider");
        assert_eq!(info.ext_kind, "ProviderExtension");

        let file = File::open(&out).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<_> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();

        assert!(
            names.iter().any(|n| n == "describe.json"),
            "missing describe.json"
        );
        assert!(
            names.iter().any(|n| n == "extension.wasm"),
            "missing extension.wasm"
        );
        assert!(
            names.iter().any(|n| n == "runtime/provider.gtpack"),
            "missing runtime/provider.gtpack; entries: {names:?}"
        );

        // Verify byte content is preserved intact.
        let mut archive = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let mut entry = archive.by_name("runtime/provider.gtpack").unwrap();
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
        assert_eq!(buf, gtpack_bytes);
    }

    #[test]
    fn provider_pack_fails_when_runtime_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let gtpack_bytes = b"fake-gtpack-content-for-testing".to_vec();
        let sha = sha256_hex(&gtpack_bytes);

        // Write describe.json pointing at a non-existent file.
        write_provider_describe(
            tmp.path(),
            &serde_json::json!({
                "file": "runtime/provider.gtpack",
                "sha256": sha
            }),
        );

        let wasm_dir = tmp.path().join("target/wasm32-wasip2/debug");
        std::fs::create_dir_all(&wasm_dir).unwrap();
        let wasm = wasm_dir.join("provider.wasm");
        std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();

        let out = tmp.path().join("out.gtxpack");
        let err = build_pack(tmp.path(), &wasm, &out).unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "expected 'not found' in error; got: {err}"
        );
    }

    #[test]
    fn provider_pack_fails_when_sha256_mismatch() {
        let tmp = tempfile::tempdir().unwrap();

        std::fs::create_dir_all(tmp.path().join("runtime")).unwrap();
        std::fs::write(tmp.path().join("runtime/provider.gtpack"), b"real-content").unwrap();

        // Declare a wrong (all-zeros) sha256.
        write_provider_describe(
            tmp.path(),
            &serde_json::json!({
                "file": "runtime/provider.gtpack",
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
            }),
        );

        let wasm_dir = tmp.path().join("target/wasm32-wasip2/debug");
        std::fs::create_dir_all(&wasm_dir).unwrap();
        let wasm = wasm_dir.join("provider.wasm");
        std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();

        let out = tmp.path().join("out.gtxpack");
        let err = build_pack(tmp.path(), &wasm, &out).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("mismatch") || msg.contains("sha256"),
            "expected 'mismatch' or 'sha256' in error; got: {msg}"
        );
    }

    #[test]
    fn design_pack_unchanged_without_gtpack() {
        // DesignExtension has no runtime.gtpack field — build_pack must succeed
        // with original behavior (no extra entries beyond describe + wasm + assets).
        let tmp = tempfile::tempdir().unwrap();
        let wasm = make_project(tmp.path());
        let out = tmp.path().join("dist/demo-0.1.0.gtxpack");

        let info = build_pack(tmp.path(), &wasm, &out).unwrap();
        assert_eq!(info.ext_kind, "DesignExtension");

        let file = File::open(&out).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<_> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();

        // Only describe.json + extension.wasm, no runtime/ entry.
        assert_eq!(names.len(), 2, "expected exactly 2 entries; got: {names:?}");
        assert!(names.iter().any(|n| n == "describe.json"));
        assert!(names.iter().any(|n| n == "extension.wasm"));
    }

    #[test]
    fn provider_pack_handles_null_gtpack_field() {
        // describe.json with runtime.gtpack: null explicitly — should behave
        // like absent field (no extra entry, no error).
        let tmp = tempfile::tempdir().unwrap();

        write_provider_describe(tmp.path(), &serde_json::Value::Null);

        let wasm_dir = tmp.path().join("target/wasm32-wasip2/debug");
        std::fs::create_dir_all(&wasm_dir).unwrap();
        let wasm = wasm_dir.join("provider.wasm");
        std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();

        let out = tmp.path().join("out.gtxpack");
        let info = build_pack(tmp.path(), &wasm, &out).unwrap();

        // Must succeed with exactly 2 entries (describe + wasm).
        let file = File::open(&out).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<_> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert_eq!(
            names.len(),
            2,
            "null gtpack should produce 2 entries; got: {names:?}"
        );
        let _ = info;
    }
}
