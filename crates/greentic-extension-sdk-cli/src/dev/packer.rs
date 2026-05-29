//! `.gtxpack` builder: stages describe + wasm + assets and hands off to the
//! shared `greentic-extension-sdk-contract::pack_writer` for deterministic ZIP emission.

use std::path::{Path, PathBuf};

use greentic_extension_sdk_contract::pack_writer::{
    PackEntry, build_gtxpack_with_manifest, sha256_hex,
};
use greentic_extension_sdk_contract::{DescribeJson, bind_manifest, sign_describe};
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
    /// The final `describe.json` bytes written into the pack — manifest-bound
    /// (and signed, when a key was supplied). Publish sends these so the
    /// registry metadata matches the pack exactly.
    pub describe_bytes: Vec<u8>,
}

/// Walk `describe.runtime.components.<key>.gtpack` and append every referenced
/// file (other than `extension.wasm` and the output pack itself) to `entries`,
/// sha256-verifying each. Returns silently if the components map is absent or
/// empty (Design/Bundle/Deploy extensions with self-contained wasm).
fn collect_runtime_component_files(
    describe: &serde_json::Value,
    project_dir: &Path,
    output_pack: &Path,
    entries: &mut Vec<PackEntry>,
) -> anyhow::Result<()> {
    let output_pack_name = output_pack
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let Some(components) = describe["runtime"]
        .get("components")
        .and_then(|v| v.as_object())
    else {
        return Ok(());
    };
    let mut seen_files: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (comp_key, comp) in components {
        let Some(gtpack) = comp.get("gtpack").filter(|v| !v.is_null()) else {
            continue;
        };
        let file_rel = gtpack["file"].as_str().ok_or_else(|| {
            anyhow::anyhow!(
                "describe.runtime.components.{comp_key}.gtpack.file missing or not a string"
            )
        })?;
        if file_rel == "extension.wasm" || file_rel == output_pack_name {
            continue;
        }
        if !seen_files.insert(file_rel.to_string()) {
            continue;
        }
        let expected_sha = gtpack["sha256"].as_str().ok_or_else(|| {
            anyhow::anyhow!(
                "describe.runtime.components.{comp_key}.gtpack.sha256 missing or not a string"
            )
        })?;
        // `file_rel` comes from describe.json (author-controlled). Reject
        // absolute paths and any `..` component so a crafted describe cannot
        // make build_pack read a file outside the project dir (e.g. /etc/passwd)
        // and embed it in the published pack.
        let candidate = Path::new(file_rel);
        if candidate.is_absolute()
            || candidate
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!(
                "describe.runtime.components.{comp_key}.gtpack.file = {file_rel:?} must be a \
                 project-relative path with no `..` or absolute components"
            );
        }
        let abs = project_dir.join(file_rel);
        if !abs.exists() {
            anyhow::bail!(
                "describe.runtime.components.{comp_key}.gtpack.file = {file_rel:?} but file not found at {}.\n\
                 Multi-component extensions must stage their runtime .gtpack into the project before publish.\n\
                 For pilot/dev, ship a placeholder file at the declared path with sha256 matching describe.json.",
                abs.display()
            );
        }
        let bytes = std::fs::read(&abs)
            .map_err(|e| anyhow::anyhow!("read runtime gtpack at {}: {e}", abs.display()))?;
        let actual_sha = sha256_hex(&bytes);
        if actual_sha != expected_sha {
            anyhow::bail!(
                "describe.runtime.components.{comp_key}.gtpack.sha256 mismatch for {file_rel}:\n\
                 declared: {expected_sha}\n\
                 actual:   {actual_sha}\n\
                 Either rebuild the runtime + update describe.json, or update describe.json to match the staged file."
            );
        }
        entries.push(PackEntry::file(file_rel.to_string(), bytes));
    }
    Ok(())
}

/// Build a `.gtxpack` at `output_pack` from `project_dir` + the already-built
/// `wasm_path`. The ZIP contains `describe.json`, the wasm renamed to
/// `extension.wasm`, and any optional asset dirs that exist (`i18n/`,
/// `schemas/`, `prompts/`, `assets/`). `assets/` is what `describe.metadata.icon`
/// resolves against once the extension is unpacked.
///
/// For multi-component extensions (v2 schema), each entry in
/// `describe.runtime.components.<key>.gtpack` whose `file` is not
/// `extension.wasm` is read, sha256-verified, and embedded in the archive at
/// the declared project-relative path (e.g. `runtime/provider.gtpack`).
pub fn build_pack(
    project_dir: &Path,
    wasm_path: &Path,
    output_pack: &Path,
) -> anyhow::Result<PackInfo> {
    build_pack_with_key(project_dir, wasm_path, output_pack, None)
}

/// Read the raw `manifest.json` bytes from a freshly-built `.gtxpack`.
fn read_manifest_from_zip(zip_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;
    let mut entry = archive
        .by_name("manifest.json")
        .map_err(|e| anyhow::anyhow!("packer produced no manifest.json: {e}"))?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Like [`build_pack`], but binds the whole-archive manifest into `describe.json`
/// (`manifestSha256`) and, when `signing_key` is `Some`, signs the bound
/// describe. The binding is keyless and always applied so the produced pack
/// passes manifest-binding verification; signing is what makes it install under
/// the Normal/Strict trust policies.
pub fn build_pack_with_key(
    project_dir: &Path,
    wasm_path: &Path,
    output_pack: &Path,
    signing_key: Option<&ed25519_dalek::SigningKey>,
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

    collect_runtime_component_files(&describe, project_dir, output_pack, &mut entries)?;

    for asset_dir in ["i18n", "schemas", "prompts", "assets"] {
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
    // Pass 1: pack to obtain the canonical `manifest.json` bytes the writer
    // produces (post-normalization, sorted) — the exact bytes a verifier will
    // hash. `manifest.json` excludes `describe.json`, so editing describe below
    // does not change it on the second pass.
    let zip1 = build_gtxpack_with_manifest(entries.clone())
        .map_err(|e| anyhow::anyhow!("build_gtxpack_with_manifest: {e}"))?;
    let manifest_bytes = read_manifest_from_zip(&zip1)?;

    // Bind the manifest into describe (and sign when a key is supplied), then
    // re-emit describe.json so the pack carries an authenticated descriptor.
    let mut describe_typed: DescribeJson = serde_json::from_value(describe.clone())
        .map_err(|e| anyhow::anyhow!("parse describe.json as typed: {e}"))?;
    bind_manifest(&mut describe_typed, &manifest_bytes);
    if let Some(key) = signing_key {
        sign_describe(&mut describe_typed, key).map_err(|e| anyhow::anyhow!("sign: {e}"))?;
    }
    let final_describe_bytes = serde_json::to_vec_pretty(&describe_typed)?;
    if let Some(entry) = entries.iter_mut().find(|e| e.path == "describe.json") {
        entry.bytes.clone_from(&final_describe_bytes);
    }

    // Pass 2: final pack with the bound/signed describe.json.
    let zip_bytes = build_gtxpack_with_manifest(entries)
        .map_err(|e| anyhow::anyhow!("build_gtxpack_with_manifest: {e}"))?;
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
        describe_bytes: final_describe_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn make_project(root: &Path) -> PathBuf {
        let desc = br#"{
  "apiVersion": "greentic.ai/v2",
  "kind": "DesignExtension",
  "compat": {"min_designer_version": ">=1.0.0", "min_runner_version": "^0.12.0", "contract_version": "1.2.4-research"},
  "metadata": {"id": "com.example.demo", "name": "demo", "version": "0.1.0", "summary": "x", "author": {"name": "a"}, "license": "Apache-2.0"},
  "engine": {"greenticDesigner": "^0.1.0", "extRuntime": "^0.1.0"},
  "capabilities": {"offered": [], "required": []},
  "runtime": {"components": {"stub": {"oci_ref": "oci://ghcr.io/example/stub:latest", "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "world": "greentic:component/stub@0.1.0"}}, "permissions": {"network": [], "secrets": [], "callExtensionKinds": []}},
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
    fn build_pack_with_key_produces_verifiable_signed_pack() {
        let tmp = tempfile::tempdir().unwrap();
        let wasm = make_project(tmp.path());
        let out = tmp.path().join("dist/demo-0.1.0.gtxpack");
        let sk = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);

        let info = build_pack_with_key(tmp.path(), &wasm, &out, Some(&sk)).unwrap();
        let zip_bytes = std::fs::read(&out).unwrap();
        let manifest_bytes = read_manifest_from_zip(&zip_bytes).unwrap();
        let describe: DescribeJson = serde_json::from_slice(&info.describe_bytes).unwrap();

        // The produced pack must pass exactly the checks the registry runs at
        // install: archive integrity, manifest binding, anchored authenticity.
        greentic_extension_sdk_contract::verify_archive_against_manifest(&zip_bytes).unwrap();
        greentic_extension_sdk_contract::verify_manifest_binding(&describe, &manifest_bytes)
            .unwrap();
        greentic_extension_sdk_contract::verify_describe_with_key(&describe, &sk.verifying_key())
            .unwrap();
    }

    #[test]
    fn build_pack_unsigned_is_manifest_bound_but_unsigned() {
        let tmp = tempfile::tempdir().unwrap();
        let wasm = make_project(tmp.path());
        let out = tmp.path().join("dist/demo-0.1.0.gtxpack");

        let info = build_pack(tmp.path(), &wasm, &out).unwrap();
        let describe: DescribeJson = serde_json::from_slice(&info.describe_bytes).unwrap();
        assert!(
            describe.manifest_sha256.is_some(),
            "manifest must be bound even without a signing key"
        );
        assert!(describe.signature.is_none(), "no key → no signature");

        let zip_bytes = std::fs::read(&out).unwrap();
        let manifest_bytes = read_manifest_from_zip(&zip_bytes).unwrap();
        greentic_extension_sdk_contract::verify_manifest_binding(&describe, &manifest_bytes)
            .unwrap();
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

    fn describe_with_gtpack_file(file: &str) -> serde_json::Value {
        serde_json::json!({
            "runtime": { "components": {
                "comp": { "gtpack": {
                    "file": file,
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
                }}
            }}
        })
    }

    #[test]
    fn collect_runtime_files_rejects_parent_dir_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let describe = describe_with_gtpack_file("../escape.gtpack");
        let mut entries = Vec::new();
        let err = collect_runtime_component_files(
            &describe,
            tmp.path(),
            &tmp.path().join("out.gtxpack"),
            &mut entries,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("project-relative"),
            "expected a path-traversal rejection, got: {err}"
        );
    }

    #[test]
    fn collect_runtime_files_rejects_absolute_path() {
        let tmp = tempfile::tempdir().unwrap();
        let describe = describe_with_gtpack_file("/etc/passwd");
        let mut entries = Vec::new();
        let err = collect_runtime_component_files(
            &describe,
            tmp.path(),
            &tmp.path().join("out.gtxpack"),
            &mut entries,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("project-relative"),
            "expected a path-traversal rejection, got: {err}"
        );
    }

    #[test]
    fn collect_runtime_files_accepts_legit_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = b"runtime-pack-bytes";
        std::fs::create_dir_all(tmp.path().join("runtime")).unwrap();
        std::fs::write(tmp.path().join("runtime/p.gtpack"), bytes).unwrap();
        let describe = serde_json::json!({
            "runtime": { "components": {
                "comp": { "gtpack": { "file": "runtime/p.gtpack", "sha256": sha256_hex(bytes) }}
            }}
        });
        let mut entries = Vec::new();
        collect_runtime_component_files(
            &describe,
            tmp.path(),
            &tmp.path().join("out.gtxpack"),
            &mut entries,
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
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
    fn build_pack_includes_icon_assets_dir() {
        // describe.metadata.icon resolves to `assets/<file>` after unpack;
        // the consuming runtime serves it from `<extension-dir>/<icon-rel>`,
        // so the dir must ship inside the gtxpack zip.
        let tmp = tempfile::tempdir().unwrap();
        let wasm = make_project(tmp.path());
        std::fs::create_dir_all(tmp.path().join("assets")).unwrap();
        std::fs::write(
            tmp.path().join("assets/icon.svg"),
            br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"/>"#,
        )
        .unwrap();
        let out = tmp.path().join("demo.gtxpack");
        build_pack(tmp.path(), &wasm, &out).unwrap();
        let file = File::open(&out).unwrap();
        let zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<_> = zip.file_names().map(str::to_string).collect();
        assert!(
            names.iter().any(|n| n == "assets/icon.svg"),
            "assets/icon.svg missing from gtxpack; entries: {names:?}"
        );
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

    /// Write a minimal provider describe.json with a single `provider` component
    /// whose `gtpack` block uses `gtpack_field` (or `null` to test the absent case).
    fn write_provider_describe(root: &Path, gtpack_field: &serde_json::Value) {
        // Every component needs a top-level sha256 + world to be a valid
        // `RuntimeComponent`. Absent-gtpack uses oci_ref (the offline-fallback
        // gtpack is simply not declared); present-gtpack carries the full
        // `RuntimeGtpack` block (file, sha256, pack_id, component_version).
        const ZERO_SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";
        let component = if gtpack_field.is_null() {
            serde_json::json!({
                "oci_ref": "oci://ghcr.io/example/provider:latest",
                "sha256": ZERO_SHA,
                "world": "greentic:component/provider@0.1.0"
            })
        } else {
            // Augment the caller's {file, sha256} block with the fields the
            // contract requires so the describe parses as typed.
            let mut gtpack = gtpack_field.clone();
            let obj = gtpack.as_object_mut().expect("gtpack block is an object");
            obj.entry("pack_id")
                .or_insert_with(|| serde_json::json!("com.example.provider.runtime"));
            obj.entry("component_version")
                .or_insert_with(|| serde_json::json!("0.1.0"));
            serde_json::json!({
                "gtpack": gtpack,
                "sha256": ZERO_SHA,
                "world": "greentic:component/provider@0.1.0"
            })
        };
        let desc = serde_json::json!({
            "apiVersion": "greentic.ai/v2",
            "kind": "ProviderExtension",
            "compat": {
                "min_designer_version": ">=1.0.0",
                "min_runner_version": "^0.12.0",
                "contract_version": "1.2.4-research"
            },
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
                "components": { "provider": component },
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

        // describe.json + extension.wasm + manifest.json (D.4.2: every
        // pack now carries a whole-archive integrity manifest); no
        // runtime/ entry.
        assert_eq!(
            names.len(),
            3,
            "expected describe + wasm + manifest; got: {names:?}"
        );
        assert!(names.iter().any(|n| n == "describe.json"));
        assert!(names.iter().any(|n| n == "extension.wasm"));
        assert!(names.iter().any(|n| n == "manifest.json"));
    }

    #[test]
    fn provider_pack_handles_absent_gtpack_field() {
        // Component without a gtpack key — should behave like the design case
        // (no extra entry, no error).
        let tmp = tempfile::tempdir().unwrap();

        write_provider_describe(tmp.path(), &serde_json::Value::Null);

        let wasm_dir = tmp.path().join("target/wasm32-wasip2/debug");
        std::fs::create_dir_all(&wasm_dir).unwrap();
        let wasm = wasm_dir.join("provider.wasm");
        std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();

        let out = tmp.path().join("out.gtxpack");
        let info = build_pack(tmp.path(), &wasm, &out).unwrap();

        // describe + wasm + manifest.json (D.4.2: every pack now carries a
        // whole-archive integrity manifest).
        let file = File::open(&out).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<_> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert_eq!(
            names.len(),
            3,
            "absent gtpack should produce describe + wasm + manifest; got: {names:?}"
        );
        let _ = info;
    }
}
