//! Provider fixture helpers for integration tests.
//!
//! Shared by both `greentic-extension-sdk-registry` and `greentic-extension-sdk-cli` test suites.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use greentic_extension_sdk_contract::{
    Compat, DescribeJson, ExtensionKind, LocalizedString, RuntimeGtpack,
    describe::{Author, Capabilities, Contributions, Engine, Metadata, Permissions, Runtime},
    hex,
};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

/// Compute a SHA-256 hex string for the given bytes.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(&Sha256::digest(bytes))
}

/// Build a minimal `.gtxpack` ZIP containing `describe.json` and the embedded
/// `.gtpack` file at `runtime/provider.gtpack`.
///
/// The `describe.json` is serialized from a real [`DescribeJson`] struct so it
/// always round-trips correctly.
///
/// # Errors
/// Returns an error if `sha256` is not a valid 64-char hex digest, or if the
/// archive cannot be created or written.
pub fn build_provider_fixture_gtxpack(
    staging_root: &Path,
    id: &str,
    version: &str,
    gtpack_bytes: &[u8],
    sha256: &str,
) -> Result<PathBuf> {
    let out = staging_root.join(format!("{id}-{version}.gtxpack"));
    let file = std::fs::File::create(&out)
        .with_context(|| format!("create fixture gtxpack {}", out.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let gtpack = RuntimeGtpack {
        file: "runtime/provider.gtpack".into(),
        sha256: sha256.into(),
        pack_id: id.into(),
        component_version: "0.6.0".into(),
    };
    let component = greentic_extension_sdk_contract::RuntimeComponent {
        oci_ref: None,
        gtpack: Some(gtpack),
        sha256: sha256
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid sha256 fixture {sha256:?}: {e}"))?,
        world: "greentic:component/stub@0.1.0".into(),
    };
    let mut components = std::collections::BTreeMap::new();
    let component_id = "stub"
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid component id: {e}"))?;
    components.insert(component_id, component);

    let mut describe = DescribeJson {
        secret_requirements: Vec::new(),
        schema_ref: None,
        api_version: "greentic.ai/v2".into(),
        kind: ExtensionKind::Provider,
        compat: Compat {
            min_designer_version: ">=1.0.0".parse().context("min_designer_version")?,
            min_runner_version: "^0.12.0".parse().context("min_runner_version")?,
            contract_version: "1.2.0".parse().context("contract_version")?,
        },
        metadata: Metadata {
            id: id.into(),
            name: id.into(),
            version: version.into(),
            summary: LocalizedString::plain("fixture"),
            description: None,
            author: Author {
                name: "Fixture".into(),
                email: None,
                public_key: None,
            },
            license: "MIT".into(),
            homepage: None,
            repository: None,
            keywords: vec![],
            icon: None,
            screenshots: vec![],
        },
        engine: Some(Engine {
            greentic_designer: "*".into(),
            ext_runtime: "^0.1.0".into(),
        }),
        capabilities: Capabilities {
            offered: vec![],
            required: vec![],
        },
        runtime: Runtime {
            world: None,
            memory_limit_mb: 64,
            permissions: Permissions::default(),
            components,
        },
        execution: None,
        contributions: Contributions::default(),
        localization: None,
        signature: None,
        manifest_sha256: None,
        required_secrets: vec![],
        config_schema: None,
    };

    // The install path enforces whole-archive integrity unconditionally (audit
    // P0-3): every archive must carry a manifest.json ledger that the describe
    // commits to via manifestSha256. Build that ledger over the payload entries
    // (describe.json and manifest.json are excluded by build_manifest), bind it
    // into the describe, then emit a self-consistent archive.
    let stub_wasm: &[u8] = b"";
    let manifest = greentic_extension_sdk_contract::build_manifest(vec![
        ("wasm/stub.wasm", stub_wasm),
        ("runtime/provider.gtpack", gtpack_bytes),
    ]);
    let manifest_json = serde_json::to_vec(&manifest).context("serialize manifest")?;
    greentic_extension_sdk_contract::bind_manifest(&mut describe, &manifest_json);

    let describe_json = serde_json::to_string_pretty(&describe).context("serialize describe")?;
    zip.start_file("describe.json", opts)
        .context("zip describe.json")?;
    zip.write_all(describe_json.as_bytes())
        .context("write describe.json")?;

    zip.start_file(greentic_extension_sdk_contract::MANIFEST_ENTRY_NAME, opts)
        .context("zip manifest.json")?;
    zip.write_all(&manifest_json)
        .context("write manifest.json")?;

    zip.start_file("wasm/stub.wasm", opts)
        .context("zip wasm/stub.wasm")?;
    zip.write_all(stub_wasm).context("write wasm/stub.wasm")?;

    zip.start_file("runtime/provider.gtpack", opts)
        .context("zip runtime/provider.gtpack")?;
    zip.write_all(gtpack_bytes)
        .context("write runtime/provider.gtpack")?;

    zip.finish().context("finish gtxpack zip")?;
    Ok(out)
}

/// Build a minimal `.gtpack` ZIP whose `manifest.cbor` entry contains
/// `{"pack_id": pack_id, "version": "0.1.0"}` encoded in CBOR.
///
/// # Errors
/// Returns an error if the CBOR cannot be encoded or the archive cannot be
/// written.
pub fn encode_gtpack_with_pack_id(pack_id: &str) -> Result<Vec<u8>> {
    let manifest = serde_json::json!({ "pack_id": pack_id, "version": "0.1.0" });
    let mut cbor_bytes = Vec::new();
    ciborium::into_writer(&manifest, &mut cbor_bytes).context("encode manifest.cbor")?;

    let mut buf = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zip = zip::ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("manifest.cbor", opts)
            .context("zip manifest.cbor")?;
        zip.write_all(&cbor_bytes).context("write manifest.cbor")?;
        zip.finish().context("finish gtpack zip")?;
    }
    Ok(buf)
}
