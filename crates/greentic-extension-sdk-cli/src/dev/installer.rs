//! Wraps `greentic-extension-sdk-registry::Installer` with a `LocalFilesystemRegistry`.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use greentic_extension_sdk_registry::lifecycle::{InstallOptions, Installer, TrustPolicy};
use greentic_extension_sdk_registry::local::LocalFilesystemRegistry;
use greentic_extension_sdk_registry::storage::Storage;

use super::packer::PackInfo;

/// Install a `.gtxpack` into the given `home` by copying it into a staging
/// filesystem registry and invoking the standard `Installer`.
pub async fn install_pack(home: &Path, pack: &PackInfo) -> anyhow::Result<InstallSummary> {
    let registry_dir = home.join("registries/dev-local");
    std::fs::create_dir_all(&registry_dir)
        .with_context(|| format!("create registry dir {}", registry_dir.display()))?;
    // Key the staged pack by `describe.metadata.id`, not the display name.
    // This staging registry is a flat scratch dir keyed purely by filename, so
    // writer and reader must agree on the same string — and `Installer::install`
    // takes an extension *id*, which it now checks against the id in the served
    // describe. Passing `metadata.name` here (free-form display text) used to
    // work only because nothing compared the two. The id is schema-constrained
    // to a safe single path component; it is sanitized anyway as belt and
    // braces, since the staging path is derived from it.
    // `pack.ext_name` is kept for `InstallSummary.name`, which is display-only.
    let safe_name = greentic_extension_sdk_contract::sanitize_filename_component(&pack.ext_id);
    let staged_pack = registry_dir.join(format!("{safe_name}-{}.gtxpack", pack.ext_version));
    copy_atomic(&pack.pack_path, &staged_pack)
        .with_context(|| format!("stage pack at {}", staged_pack.display()))?;

    let storage = Storage::new(home);
    let reg = LocalFilesystemRegistry::new("dev-local", registry_dir.clone());
    let installer = Installer::new(storage.clone_shallow(), &reg);
    installer
        .install(
            &safe_name,
            &pack.ext_version,
            InstallOptions {
                trust_policy: TrustPolicy::Loose,
                accept_permissions: true,
                force: false,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    warn_if_designer_cannot_load(&pack.describe_bytes, &pack.ext_name);

    Ok(InstallSummary {
        registry: registry_dir,
        name: pack.ext_name.clone(),
        version: pack.ext_version.clone(),
    })
}

/// Tell the author straight after an install when the designer on this machine
/// will not load what was just installed.
///
/// The inner loop otherwise reports a clean install and the extension then
/// silently never appears in Designer — which is the failure this whole check
/// exists for. Best-effort: an unparseable describe or no local designer says
/// nothing, because neither is a compatibility problem.
pub(crate) fn warn_if_designer_cannot_load(describe_bytes: &[u8], label: &str) {
    let Ok(describe) = serde_json::from_slice::<serde_json::Value>(describe_bytes) else {
        return;
    };
    let designer = crate::commands::doctor::designer_compat::installed_designer_version();
    if let Some(warning) =
        crate::commands::doctor::designer_compat::install_warning(designer.as_ref(), &describe)
    {
        eprintln!("⚠ {label} {warning}");
    }
}

#[derive(Debug, Clone)]
pub struct InstallSummary {
    pub registry: PathBuf,
    #[allow(dead_code)]
    pub name: String,
    pub version: String,
}

fn copy_atomic(src: &Path, dst: &Path) -> std::io::Result<()> {
    let tmp = dst.with_extension("gtxpack.tmp");
    std::fs::copy(src, &tmp)?;
    if dst.exists() {
        std::fs::remove_file(dst)?;
    }
    std::fs::rename(&tmp, dst)?;
    sync_parent_dir(dst);
    Ok(())
}

/// Best-effort fsync of `path`'s parent directory so the rename above is
/// durable across a crash (same pattern as `greentic-extension-sdk-state`'s
/// atomic writer). A failure here never fails the install — the file content
/// itself is already safely copied.
fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent()
        && let Ok(dir) = std::fs::File::open(parent)
    {
        let _ = dir.sync_all();
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_pack_named(tmp: &Path, ext_name: &str) -> (PathBuf, PackInfo) {
        let pack = tmp.join("demo-0.1.0.gtxpack");
        let file = std::fs::File::create(&pack).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let desc = br#"{
"apiVersion":"greentic.ai/v2","kind":"DesignExtension",
"compat":{"min_designer_version":">=1.0.0","min_runner_version":"^0.12.0","contract_version":"1.2.0"},
"metadata":{"id":"com.example.demo","name":"demo","version":"0.1.0","summary":"x","author":{"name":"a"},"license":"Apache-2.0"},
"engine":{"greenticDesigner":"^0.1","extRuntime":"^0.1"},
"capabilities":{"offered":[],"required":[]},
"runtime":{"memoryLimitMB":64,"permissions":{"network":[],"secrets":[],"callExtensionKinds":[]},"components":{"stub":{"oci_ref":"oci://ghcr.io/example/stub:latest","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","world":"greentic:component/stub@0.1.0"}}},
"contributions":{}}"#;
        let wasm = b"\0asm\x01\x00\x00\x00";

        // The install path enforces whole-archive integrity unconditionally
        // (audit P0-3): every archive must carry a manifest.json ledger that the
        // describe commits to via manifestSha256. Build that ledger over the
        // payload, bind it into the describe, then write a self-consistent
        // archive — otherwise install rejects this fixture even under Loose.
        let manifest = greentic_extension_sdk_contract::build_manifest(vec![(
            "extension.wasm",
            wasm.as_slice(),
        )]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut describe: greentic_extension_sdk_contract::DescribeJson =
            serde_json::from_slice(desc).unwrap();
        greentic_extension_sdk_contract::bind_manifest(&mut describe, &manifest_json);
        let desc = serde_json::to_vec(&describe).unwrap();

        zip.start_file("describe.json", opts).unwrap();
        zip.write_all(&desc).unwrap();
        zip.start_file("extension.wasm", opts).unwrap();
        zip.write_all(wasm).unwrap();
        zip.start_file(greentic_extension_sdk_contract::MANIFEST_ENTRY_NAME, opts)
            .unwrap();
        zip.write_all(&manifest_json).unwrap();
        zip.finish().unwrap();

        let info = PackInfo {
            pack_path: pack.clone(),
            pack_name: "demo-0.1.0.gtxpack".into(),
            size: std::fs::metadata(&pack).unwrap().len(),
            sha256: "dummy".into(),
            ext_name: ext_name.into(),
            // Identity comes from describe.metadata.id, independent of the
            // display name the caller passed in.
            ext_id: "com.example.demo".into(),
            ext_version: "0.1.0".into(),
            ext_kind: "design".into(),
            describe_bytes: desc.clone(),
        };
        (pack, info)
    }

    fn sample_pack(tmp: &Path) -> (PathBuf, PackInfo) {
        sample_pack_named(tmp, "demo")
    }

    #[tokio::test]
    async fn install_pack_creates_extension_dir_in_storage() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let src_dir = tmp.path().join("dist");
        std::fs::create_dir_all(&src_dir).unwrap();
        let (_src, info) = sample_pack(&src_dir);

        let summary = install_pack(&home, &info).await.expect("install");
        assert_eq!(summary.name, "demo");
        assert_eq!(summary.version, "0.1.0");
        // Storage lays out extensions at <home>/extensions/<kind>/<id>-<version>/.
        // Note: the lifecycle::Installer uses `describe.metadata.id` (not `.name`)
        // when deciding the final directory, so the install path reflects the id.
        let expected = home.join("extensions/design/com.example.demo-0.1.0");
        assert!(expected.exists(), "expected {}", expected.display());
        assert!(expected.join("describe.json").exists());
        assert!(expected.join("extension.wasm").exists());
    }

    /// Empirical repro (`gtdx dev` variant): `pack.ext_name` containing "/"
    /// (e.g. "Topic / scope guardrail") must not crash the staging write,
    /// AND the sanitized name used to write the staged file must be the
    /// same one used to look it back up via `Installer::install` — otherwise
    /// the write would silently "succeed" while install then fails to find
    /// what it just staged.
    #[tokio::test]
    async fn install_pack_with_slash_in_name_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let src_dir = tmp.path().join("dist");
        std::fs::create_dir_all(&src_dir).unwrap();
        let (_src, info) = sample_pack_named(&src_dir, "Topic / scope guardrail");

        let summary = install_pack(&home, &info)
            .await
            .expect("install must succeed despite '/' in ext_name");
        assert_eq!(summary.version, "0.1.0");

        let expected = home.join("extensions/design/com.example.demo-0.1.0");
        assert!(expected.exists(), "expected {}", expected.display());
        assert!(expected.join("describe.json").exists());
        assert!(expected.join("extension.wasm").exists());

        // The staged pack itself must be a single path component directly
        // under the registry dir, not a nested "Topic " subdirectory.
        let registry_dir = home.join("registries/dev-local");
        let staged: Vec<_> = std::fs::read_dir(&registry_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .collect();
        assert_eq!(
            staged.len(),
            1,
            "expected exactly one staged file directly under {}",
            registry_dir.display()
        );
    }
}
