//! Wraps `greentic-extension-sdk-registry::Installer` with a `LocalFilesystemRegistry`.

use std::path::{Path, PathBuf};

use greentic_extension_sdk_registry::lifecycle::{InstallOptions, Installer, TrustPolicy};
use greentic_extension_sdk_registry::local::LocalFilesystemRegistry;
use greentic_extension_sdk_registry::storage::Storage;

use super::packer::PackInfo;

/// Install a `.gtxpack` into the given `home` by copying it into a staging
/// filesystem registry and invoking the standard `Installer`.
pub async fn install_pack(home: &Path, pack: &PackInfo) -> anyhow::Result<InstallSummary> {
    let registry_dir = home.join("registries/dev-local");
    std::fs::create_dir_all(&registry_dir)?;
    let staged_pack = registry_dir.join(format!("{}-{}.gtxpack", pack.ext_name, pack.ext_version));
    copy_atomic(&pack.pack_path, &staged_pack)?;

    let storage = Storage::new(home);
    let reg = LocalFilesystemRegistry::new("dev-local", registry_dir.clone());
    let installer = Installer::new(storage.clone_shallow(), &reg);
    installer
        .install(
            &pack.ext_name,
            &pack.ext_version,
            InstallOptions {
                trust_policy: TrustPolicy::Loose,
                accept_permissions: true,
                force: false,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(InstallSummary {
        registry: registry_dir,
        name: pack.ext_name.clone(),
        version: pack.ext_version.clone(),
    })
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_pack(tmp: &Path) -> (PathBuf, PackInfo) {
        let pack = tmp.join("demo-0.1.0.gtxpack");
        let file = std::fs::File::create(&pack).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let desc = br#"{
"apiVersion":"greentic.ai/v1","kind":"DesignExtension",
"metadata":{"id":"com.example.demo","name":"demo","version":"0.1.0","summary":"x","author":{"name":"a"},"license":"Apache-2.0"},
"engine":{"greenticDesigner":"^0.1","extRuntime":"^0.1"},
"capabilities":{"offered":[],"required":[]},
"runtime":{"component":"extension.wasm","permissions":{"network":[],"secrets":[],"callExtensionKinds":[]}},
"contributions":{}}"#;
        zip.start_file("describe.json", opts).unwrap();
        zip.write_all(desc).unwrap();
        zip.start_file("extension.wasm", opts).unwrap();
        zip.write_all(b"\0asm\x01\x00\x00\x00").unwrap();
        zip.finish().unwrap();

        let info = PackInfo {
            pack_path: pack.clone(),
            pack_name: "demo-0.1.0.gtxpack".into(),
            size: std::fs::metadata(&pack).unwrap().len(),
            sha256: "dummy".into(),
            ext_name: "demo".into(),
            ext_version: "0.1.0".into(),
            ext_kind: "design".into(),
        };
        (pack, info)
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
}
