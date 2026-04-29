use std::path::Path;
use std::process::Command;

use greentic_extension_sdk_contract::{
    CapabilityId, CapabilityRef, DescribeJson, ExtensionKind,
    describe::{Author, Capabilities, Engine, Metadata, Permissions, Runtime, RuntimeGtpack},
};
use greentic_extension_sdk_registry::hex;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(&digest)
}

fn write_design_fixture(extensions_root: &std::path::Path) {
    let design_dir = extensions_root
        .join("design")
        .join("greentic.design.adaptive-cards-0.1.0");
    std::fs::create_dir_all(&design_dir).unwrap();

    let design_describe = DescribeJson {
        schema_ref: None,
        api_version: "greentic.ai/v1".into(),
        kind: ExtensionKind::Design,
        metadata: Metadata {
            id: "greentic.design.adaptive-cards".into(),
            name: "Adaptive Cards".into(),
            version: "0.1.0".into(),
            summary: "Design extension for adaptive cards".into(),
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
        engine: Engine {
            greentic_designer: "*".into(),
            ext_runtime: "*".into(),
        },
        capabilities: Capabilities {
            offered: vec![],
            required: vec![],
        },
        runtime: Runtime {
            component: "extension.wasm".into(),
            memory_limit_mb: 64,
            permissions: Permissions::default(),
            gtpack: None,
        },
        execution: None,
        contributions: serde_json::json!({}),
        signature: None,
    };

    let describe_path = design_dir.join("describe.json");
    std::fs::write(
        &describe_path,
        serde_json::to_string_pretty(&design_describe).unwrap(),
    )
    .unwrap();
}

fn write_provider_fixture(extensions_root: &std::path::Path) {
    let provider_dir = extensions_root
        .join("provider")
        .join("greentic.provider.telegram-0.2.0");
    std::fs::create_dir_all(&provider_dir).unwrap();

    let gtpack_bytes = b"fake-gtpack-data".to_vec();
    let sha256 = sha256_hex(&gtpack_bytes);

    let provider_describe = DescribeJson {
        schema_ref: None,
        api_version: "greentic.ai/v1".into(),
        kind: ExtensionKind::Provider,
        metadata: Metadata {
            id: "greentic.provider.telegram".into(),
            name: "Telegram Provider".into(),
            version: "0.2.0".into(),
            summary: "Provider extension for Telegram".into(),
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
        engine: Engine {
            greentic_designer: "*".into(),
            ext_runtime: "^0.1.0".into(),
        },
        capabilities: Capabilities {
            offered: vec![],
            required: vec![],
        },
        runtime: Runtime {
            component: "wasm/provider.wasm".into(),
            memory_limit_mb: 256,
            permissions: Permissions::default(),
            gtpack: Some(RuntimeGtpack {
                file: "runtime/provider.gtpack".into(),
                sha256,
                pack_id: "greentic.provider.telegram".into(),
                component_version: "0.6.0".into(),
            }),
        },
        execution: None,
        contributions: serde_json::json!({}),
        signature: None,
    };

    let describe_path = provider_dir.join("describe.json");
    std::fs::write(
        &describe_path,
        serde_json::to_string_pretty(&provider_describe).unwrap(),
    )
    .unwrap();
}

fn setup_fixture_extensions(home: &Path) {
    let extensions_root = home.join("extensions");
    std::fs::create_dir_all(&extensions_root).unwrap();
    write_design_fixture(&extensions_root);
    write_provider_fixture(&extensions_root);
}

#[test]
fn gtdx_list_filters_by_kind_provider() {
    let tmp = TempDir::new().unwrap();
    setup_fixture_extensions(tmp.path());

    let output = Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "list",
            "--kind",
            "provider",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain provider extension
    assert!(
        stdout.contains("greentic.provider.telegram"),
        "stdout: {stdout}"
    );

    // Should NOT contain design extension
    assert!(
        !stdout.contains("greentic.design.adaptive-cards"),
        "stdout: {stdout}"
    );

    // Should show [provider] header
    assert!(stdout.contains("[provider]"), "stdout: {stdout}");
}

#[test]
fn gtdx_list_filters_by_kind_design() {
    let tmp = TempDir::new().unwrap();
    setup_fixture_extensions(tmp.path());

    let output = Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "list",
            "--kind",
            "design",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain design extension
    assert!(
        stdout.contains("greentic.design.adaptive-cards"),
        "stdout: {stdout}"
    );

    // Should NOT contain provider extension
    assert!(
        !stdout.contains("greentic.provider.telegram"),
        "stdout: {stdout}"
    );

    // Should show [design] header
    assert!(stdout.contains("[design]"), "stdout: {stdout}");
}

#[test]
fn gtdx_list_shows_all_kinds_by_default() {
    let tmp = TempDir::new().unwrap();
    setup_fixture_extensions(tmp.path());

    let output = Command::new(gtdx_bin())
        .args(["--home", tmp.path().to_str().unwrap(), "list"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain both design and provider extensions
    assert!(
        stdout.contains("greentic.design.adaptive-cards"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("greentic.provider.telegram"),
        "stdout: {stdout}"
    );
}

#[test]
fn gtdx_list_handles_missing_kind_dir() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("extensions")).unwrap();

    // Don't create any provider dir
    let output = Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "list",
            "--kind",
            "provider",
        ])
        .output()
        .unwrap();

    // Should succeed with empty output, not panic
    assert!(output.status.success());
}

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
        })
        .collect();

    let describe = DescribeJson {
        schema_ref: None,
        api_version: "greentic.ai/v1".into(),
        kind: ExtensionKind::Provider,
        metadata: Metadata {
            id: id.into(),
            name: "Telegram Provider".into(),
            version: version.into(),
            summary: "Provider extension for Telegram".into(),
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
        engine: Engine {
            greentic_designer: "*".into(),
            ext_runtime: "^0.1.0".into(),
        },
        capabilities: Capabilities {
            offered,
            required: vec![],
        },
        runtime: Runtime {
            component: "wasm/provider.wasm".into(),
            memory_limit_mb: 256,
            permissions: Permissions::default(),
            gtpack: Some(RuntimeGtpack {
                file: "runtime/provider.gtpack".into(),
                sha256,
                pack_id: id.into(),
                component_version: "0.6.0".into(),
            }),
        },
        execution: None,
        contributions: serde_json::json!({}),
        signature: None,
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
    );

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
