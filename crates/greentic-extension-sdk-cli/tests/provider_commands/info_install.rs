//! `gtdx info` (A9) and `gtdx install` (A10) provider tests.

use std::path::Path;
use std::process::Command;

use greentic_extension_sdk_contract::{
    CapabilityId, CapabilityRef, DescribeJson, ExtensionKind, RuntimeComponent, RuntimeGtpack,
    describe::{Author, Capabilities, Contributions, Engine, Metadata, Permissions, Runtime},
};
use tempfile::TempDir;

use crate::fixtures::{default_compat, gtdx_bin, sha256_hex};

// ---------------------------------------------------------------------------
// A9: gtdx info — local-first lookup for provider extensions
// ---------------------------------------------------------------------------

fn write_provider_fixture_with_capabilities(
    home: &Path,
    id: &str,
    version: &str,
    capability_ids: &[&str],
) {
    let extensions_root = home.join("extensions");
    let provider_dir = extensions_root
        .join("provider")
        .join(format!("{id}-{version}"));
    std::fs::create_dir_all(&provider_dir).unwrap();

    let gtpack_bytes = b"fake-gtpack-data".to_vec();
    let sha256 = sha256_hex(&gtpack_bytes);

    let offered: Vec<CapabilityRef> = capability_ids
        .iter()
        .map(|cap_str| CapabilityRef {
            id: cap_str.parse::<CapabilityId>().unwrap(),
            version: "0.1.0".into(),
            deprecated: None,
        })
        .collect();

    let describe = DescribeJson {
        secret_requirements: Vec::new(),
        schema_ref: None,
        api_version: "greentic.ai/v2".into(),
        kind: ExtensionKind::Provider,
        compat: default_compat(),
        metadata: Metadata {
            id: id.into(),
            name: "Telegram Provider".into(),
            version: version.into(),
            summary: greentic_extension_sdk_contract::LocalizedString::plain(
                "Provider extension for Telegram",
            ),
            description: None,
            author: Author {
                name: "Test".into(),
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
            offered,
            required: vec![],
        },
        runtime: Runtime {
            world: None,
            memory_limit_mb: 256,
            permissions: Permissions::default(),
            components: {
                let mut m = std::collections::BTreeMap::new();
                m.insert(
                    "provider".parse().unwrap(),
                    RuntimeComponent {
                        oci_ref: None,
                        gtpack: Some(RuntimeGtpack {
                            file: "runtime/provider.gtpack".into(),
                            sha256: sha256.clone(),
                            pack_id: id.into(),
                            component_version: "0.6.0".into(),
                        }),
                        sha256: sha256.parse().unwrap(),
                        world: "greentic:component/provider@0.1.0".into(),
                    },
                );
                m
            },
        },
        execution: None,
        contributions: Contributions::default(),
        localization: None,
        signature: None,
        manifest_sha256: None,
        required_secrets: vec![],
        config_schema: None,
    };

    let describe_path = provider_dir.join("describe.json");
    std::fs::write(
        &describe_path,
        serde_json::to_string_pretty(&describe).unwrap(),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// A10: gtdx install — routes kind=Provider through lifecycle::install_artifact
// ---------------------------------------------------------------------------

#[test]
fn gtdx_install_provider_from_gtxpack_places_files() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let gtpack_bytes = b"fake-gtpack-bytes".to_vec();
    let sha = greentic_extension_sdk_testing::sha256_hex(&gtpack_bytes);
    let gtxpack = greentic_extension_sdk_testing::build_provider_fixture_gtxpack(
        tmp.path(),
        "greentic.provider.fixture",
        "0.1.0",
        &gtpack_bytes,
        &sha,
    )
    .unwrap();

    let output = std::process::Command::new(gtdx_bin())
        .args([
            "--home",
            home.to_str().unwrap(),
            "install",
            gtxpack.to_str().unwrap(),
            "-y",
            "--trust",
            "loose",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "gtdx install failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Gtpack landed in runner pickup dir (FLAT layout: {id}-{version}.gtpack)
    let installed_pack = home
        .join("runtime/packs/providers/gtdx")
        .join("greentic.provider.fixture-0.1.0.gtpack");
    assert!(
        installed_pack.exists(),
        "expected extracted gtpack at {installed_pack:?}"
    );
    assert_eq!(std::fs::read(&installed_pack).unwrap(), gtpack_bytes);

    // Metadata landed in extensions dir (FLAT layout: {id}-{version}/)
    let describe = home
        .join("extensions/provider/greentic.provider.fixture-0.1.0")
        .join("describe.json");
    assert!(describe.exists(), "expected describe.json at {describe:?}");

    // The gtpack MUST remain in the final extensions dir.
    //
    // This assertion used to require the opposite, and that requirement was the
    // defect. `build_gtxpack_with_manifest` lists every non-directory archive
    // entry in `manifest.json` — `runtime/provider.gtpack` included — and the
    // describe commits to that ledger via `manifest_sha256`. Stripping the file
    // after verifying it left the signed ledger naming a path that was not on
    // disk, and greentic-ext-runtime's `verify_dir_manifest` walks every
    // manifest entry and hard-errors on the first one missing:
    //     manifest lists missing file: runtime/provider.gtpack
    // So no gtdx-installed provider extension could load at all.
    //
    // Note the runtime does NOT reject files present on disk but absent from
    // the manifest — `verify_dir_manifest` iterates manifest entries only and
    // never walks the directory (checked against the published
    // greentic-ext-runtime 1.2.34077794832). The reason the file must stay is
    // simply that the manifest LISTS it. The exactness in the other direction
    // is an SDK-side, archive-level rule: `verify_archive_against_manifest`
    // refuses an archive entry the manifest does not name, which is what makes
    // "just drop runtime/** from the manifest" a non-fix.
    let ext_dir = home.join("extensions/provider/greentic.provider.fixture-0.1.0");
    assert!(
        ext_dir.join("runtime/provider.gtpack").exists(),
        "gtpack must stay in extensions dir — manifest.json lists it"
    );

    // Pin the invariant the runtime actually enforces, not just this one path:
    // every entry the installed ledger names must exist and hash correctly.
    let raw = std::fs::read(ext_dir.join(greentic_extension_sdk_contract::MANIFEST_ENTRY_NAME))
        .expect("installed extension must carry manifest.json");
    let manifest: greentic_extension_sdk_contract::Manifest = serde_json::from_slice(&raw).unwrap();
    assert!(
        manifest
            .entries
            .iter()
            .any(|e| e.path == "runtime/provider.gtpack"),
        "fixture must exercise the provider gtpack path; entries: {:?}",
        manifest.entries.iter().map(|e| &e.path).collect::<Vec<_>>()
    );
    for entry in &manifest.entries {
        let path = ext_dir.join(&entry.path);
        assert!(
            path.exists(),
            "manifest lists missing file: {} (the runtime refuses to load this)",
            entry.path
        );
        assert_eq!(
            sha256_hex(&std::fs::read(&path).unwrap()),
            entry.sha256,
            "manifest sha256 mismatch for {}",
            entry.path
        );
    }
}

#[test]
fn gtdx_info_displays_provider_channels() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    write_provider_fixture_with_capabilities(
        home,
        "greentic.provider.telegram",
        "0.1.0",
        &["greentic:messaging/send@0.1.0"],
    );

    let output = Command::new(gtdx_bin())
        .args([
            "--home",
            home.to_str().unwrap(),
            "info",
            "greentic.provider.telegram",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Kind: ProviderExtension"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("Capabilities: "), "stdout: {stdout}");
    assert!(stdout.contains("messaging"), "stdout: {stdout}");
    assert!(
        stdout.contains("Runtime pack: greentic.provider.telegram"),
        "stdout: {stdout}"
    );
}
