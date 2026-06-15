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

    // Gtpack MUST NOT be in final extensions dir
    let gtpack_in_ext = home
        .join("extensions/provider/greentic.provider.fixture-0.1.0")
        .join("runtime/provider.gtpack");
    assert!(
        !gtpack_in_ext.exists(),
        "gtpack must not be left in extensions dir"
    );
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
