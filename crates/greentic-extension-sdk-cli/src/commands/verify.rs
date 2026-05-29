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
    ///   - .gtxpack archive (full chain: signature + manifest binding + ledger)
    pub path: PathBuf,

    /// Base64 ed25519 public key to anchor authenticity against (an `ed25519:`
    /// prefix is accepted). With it, the signature must have been produced by
    /// this exact key (audit C1). Without it, only describe self-consistency
    /// (integrity) is checked — it proves the describe is unmodified, not who
    /// signed it.
    #[arg(long)]
    pub trusted_key: Option<String>,
}

pub fn run(args: &Args, _home: &Path) -> Result<()> {
    let describe = load_describe(&args.path)?;

    // 1. Describe signature. Anchored authenticity when --trusted-key is given
    //    (C1), else integrity-only self-consistency.
    match &args.trusted_key {
        Some(key_b64) => {
            let trusted_key = parse_verifying_key(key_b64)?;
            greentic_extension_sdk_contract::verify_describe_with_key(&describe, &trusted_key)
                .map_err(|e| anyhow::anyhow!("authenticity check failed: {e}"))?;
        }
        None => {
            greentic_extension_sdk_contract::verify_describe_self_consistent(&describe)
                .map_err(|e| anyhow::anyhow!("signature invalid: {e}"))?;
        }
    }

    // 2. For an archive, verify the rest of the C2 chain: the describe's
    //    manifest binding (manifestSha256) and the whole-archive integrity
    //    ledger (manifest.json), so a tampered wasm or smuggled file is caught,
    //    not just a tampered describe.
    if is_archive(&args.path) {
        verify_archive_chain(&args.path, &describe)?;
    } else if describe.manifest_sha256.is_some() {
        eprintln!(
            "note: describe is manifest-bound (manifestSha256 set) but the input is not an \
             archive — the binding could not be checked"
        );
    }

    let sig = describe
        .signature
        .as_ref()
        .expect("verify passed → signature present");
    let anchored = if args.trusted_key.is_some() {
        " (anchored)"
    } else {
        ""
    };
    println!(
        "OK  {} v{} signed by {}{}",
        describe.metadata.id,
        describe.metadata.version,
        &sig.public_key[..16.min(sig.public_key.len())],
        anchored,
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

/// Verify the manifest binding + whole-archive integrity ledger.
///
/// Fail-open is intentional and bounded: a describe with no `manifestSha256`
/// (legacy / design-only) and an archive with no `manifest.json` (legacy pack)
/// warn rather than fail, so verification of pre-ledger artifacts still works.
/// But a describe that *claims* a binding (`manifestSha256` set) over an archive
/// that *dropped* its `manifest.json` is a hard failure — that is a removed
/// ledger, not a legacy pack.
fn verify_archive_chain(pack_path: &Path, describe: &DescribeJson) -> Result<()> {
    let bytes =
        std::fs::read(pack_path).with_context(|| format!("read {}", pack_path.display()))?;
    let manifest = read_archive_entry(pack_path, "manifest.json")?;

    match (&describe.manifest_sha256, &manifest) {
        (Some(_), Some(manifest_bytes)) => {
            greentic_extension_sdk_contract::verify_manifest_binding(describe, manifest_bytes)
                .map_err(|e| anyhow::anyhow!("manifest binding check failed: {e}"))?;
        }
        (Some(_), None) => {
            anyhow::bail!(
                "{} describe is manifest-bound (manifestSha256 set) but the archive has no \
                 manifest.json — integrity ledger was removed",
                pack_path.display()
            );
        }
        (None, Some(_)) => {
            eprintln!(
                "warning: {} carries a manifest.json but describe.json is not manifest-bound \
                 (no manifestSha256) — the signature does not cover the ledger",
                pack_path.display()
            );
        }
        (None, None) => {
            eprintln!(
                "warning: {} has no manifest.json — integrity ledger unavailable, only \
                 describe.json signature was checked",
                pack_path.display()
            );
            return Ok(());
        }
    }

    // Reached only when a manifest.json is present: confirm every listed entry's
    // bytes match (catches a tampered/smuggled file).
    match greentic_extension_sdk_contract::verify_archive_against_manifest(&bytes) {
        // Missing is unreachable here (manifest presence checked above) but is
        // folded into success for total matching.
        Ok(()) | Err(greentic_extension_sdk_contract::ManifestError::Missing) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("archive integrity check failed: {e}")),
    }
}

/// Read a single named entry's bytes from a zip archive, or `None` if absent.
fn read_archive_entry(pack_path: &Path, name: &str) -> Result<Option<Vec<u8>>> {
    let file =
        std::fs::File::open(pack_path).with_context(|| format!("open {}", pack_path.display()))?;
    let mut zip = zip::ZipArchive::new(file).context("open zip")?;
    match zip.by_name(name) {
        Ok(mut entry) => {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .with_context(|| format!("read {name} from archive"))?;
            Ok(Some(buf))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("read {name} from archive: {e}")),
    }
}

/// Parse a base64 ed25519 public key (an optional `ed25519:` prefix is stripped).
fn parse_verifying_key(key_b64: &str) -> Result<ed25519_dalek::VerifyingKey> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let trimmed = key_b64.strip_prefix("ed25519:").unwrap_or(key_b64);
    let raw = B64
        .decode(trimmed.trim())
        .map_err(|e| anyhow::anyhow!("--trusted-key is not valid base64: {e}"))?;
    let arr: [u8; 32] = raw
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("--trusted-key must decode to 32 bytes"))?;
    ed25519_dalek::VerifyingKey::from_bytes(&arr)
        .map_err(|e| anyhow::anyhow!("--trusted-key is not a valid ed25519 public key: {e}"))
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
    let bytes = read_archive_entry(pack_path, "describe.json")?
        .ok_or_else(|| anyhow::anyhow!("describe.json missing from archive"))?;
    serde_json::from_slice(&bytes).context("parse describe.json")
}
