use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use greentic_extension_sdk_contract::DescribeJson;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to verify. Accepts:
    ///   - describe.json file (verifies inline signature)
    ///   - extension directory (reads describe.json inside)
    ///   - .gtxpack archive (unzips describe.json to temp, verifies)
    pub path: PathBuf,
}

pub fn run(args: &Args, _home: &Path) -> Result<()> {
    let describe = load_describe(&args.path)?;
    greentic_extension_sdk_contract::verify_describe(&describe)
        .map_err(|e| anyhow::anyhow!("signature invalid: {e}"))?;

    // The describe.json signature only covers describe.json. For an archive,
    // also verify the whole-archive integrity ledger (manifest.json) so a
    // tampered `extension.wasm` or smuggled file is caught, not just a tampered
    // describe. (Publisher trust over the manifest itself is the D.5 work.)
    if is_archive(&args.path) {
        verify_archive_integrity(&args.path)?;
    }

    let sig = describe
        .signature
        .as_ref()
        .expect("verify passed → signature present");
    println!(
        "OK  {} v{} signed by {}",
        describe.metadata.id,
        describe.metadata.version,
        &sig.public_key[..16.min(sig.public_key.len())],
    );
    Ok(())
}

/// True when `path` is a `.gtxpack`/`.zip` archive file (not a directory or
/// a bare `describe.json`).
fn is_archive(path: &Path) -> bool {
    path.is_file()
        && matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("gtxpack" | "zip")
        )
}

/// Verify the archive's `manifest.json` integrity ledger. A missing manifest is
/// reported as a warning (legacy packs predate the ledger) rather than a hard
/// failure; any hash mismatch or smuggled entry is a hard failure.
fn verify_archive_integrity(pack_path: &Path) -> Result<()> {
    let bytes =
        std::fs::read(pack_path).with_context(|| format!("read {}", pack_path.display()))?;
    match greentic_extension_sdk_contract::verify_archive_against_manifest(&bytes) {
        Ok(()) => Ok(()),
        Err(greentic_extension_sdk_contract::ManifestError::Missing) => {
            eprintln!(
                "warning: {} has no manifest.json — integrity ledger unavailable, \
                 only describe.json signature was checked",
                pack_path.display()
            );
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("archive integrity check failed: {e}")),
    }
}

fn load_describe(path: &Path) -> Result<DescribeJson> {
    if path.is_file() {
        let ext = path.extension().and_then(|s| s.to_str());
        match ext {
            Some("json") => load_describe_file(path),
            Some("gtxpack" | "zip") => load_describe_from_archive(path),
            other => {
                anyhow::bail!("unsupported file extension: {other:?} (expected .json or .gtxpack)")
            }
        }
    } else if path.is_dir() {
        load_describe_file(&path.join("describe.json"))
    } else {
        anyhow::bail!("not a file or directory: {}", path.display())
    }
}

fn load_describe_file(path: &Path) -> Result<DescribeJson> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

fn load_describe_from_archive(pack_path: &Path) -> Result<DescribeJson> {
    let file =
        std::fs::File::open(pack_path).with_context(|| format!("open {}", pack_path.display()))?;
    let mut zip = zip::ZipArchive::new(file).context("open zip")?;
    let mut entry = zip
        .by_name("describe.json")
        .context("describe.json missing from archive")?;
    let mut buf = String::new();
    entry
        .read_to_string(&mut buf)
        .context("read describe.json")?;
    serde_json::from_str(&buf).context("parse describe.json")
}
