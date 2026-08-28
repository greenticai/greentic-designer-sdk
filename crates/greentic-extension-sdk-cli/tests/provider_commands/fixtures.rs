//! Shared fixtures for the `provider_commands` integration tests.

use std::path::Path;

use greentic_extension_sdk_contract::{
    Compat, DescribeJson, ExtensionKind, RuntimeComponent, RuntimeGtpack,
    describe::{Author, Capabilities, Contributions, Engine, Metadata, Permissions, Runtime},
};
use greentic_extension_sdk_registry::hex;
use sha2::{Digest, Sha256};

pub fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(&digest)
}

pub fn default_compat() -> Compat {
    Compat {
        min_designer_version: ">=1.0.0".parse().unwrap(),
        min_runner_version: "^0.12.0".parse().unwrap(),
        contract_version: "1.2.0".parse().unwrap(),
    }
}

fn write_design_fixture(extensions_root: &std::path::Path) {
    let design_dir = extensions_root
        .join("design")
        .join("greentic.design.adaptive-cards-0.1.0");
    std::fs::create_dir_all(&design_dir).unwrap();

    let design_describe = DescribeJson {
        secret_requirements: Vec::new(),
        schema_ref: None,
        api_version: "greentic.ai/v2".into(),
        kind: ExtensionKind::Design,
        compat: default_compat(),
        metadata: Metadata {
            id: "greentic.design.adaptive-cards".into(),
            name: "Adaptive Cards".into(),
            version: "0.1.0".into(),
            summary: greentic_extension_sdk_contract::LocalizedString::plain(
                "Design extension for adaptive cards",
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
            ext_runtime: "*".into(),
        }),
        capabilities: Capabilities {
            offered: vec![],
            required: vec![],
        },
        runtime: Runtime {
            world: None,
            memory_limit_mb: 64,
            permissions: Permissions::default(),
            components: {
                let mut m = std::collections::BTreeMap::new();
                m.insert(
                    "stub".parse().unwrap(),
                    RuntimeComponent {
                        oci_ref: Some("oci://ghcr.io/example/stub:latest".into()),
                        gtpack: None,
                        sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            .parse()
                            .unwrap(),
                        world: "greentic:component/stub@0.1.0".into(),
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
        secret_requirements: Vec::new(),
        schema_ref: None,
        api_version: "greentic.ai/v2".into(),
        kind: ExtensionKind::Provider,
        compat: default_compat(),
        metadata: Metadata {
            id: "greentic.provider.telegram".into(),
            name: "Telegram Provider".into(),
            version: "0.2.0".into(),
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
            offered: vec![],
            required: vec![],
        },
        runtime: Runtime {
            world: None,
            memory_limit_mb: 256,
            permissions: Permissions::default(),
            components: {
                let mut m = std::collections::BTreeMap::new();
                m.insert(
                    "telegram".parse().unwrap(),
                    RuntimeComponent {
                        oci_ref: None,
                        gtpack: Some(RuntimeGtpack {
                            file: "runtime/provider.gtpack".into(),
                            sha256: sha256.clone(),
                            pack_id: "greentic.provider.telegram".into(),
                            component_version: "0.6.0".into(),
                        }),
                        sha256: sha256.parse().unwrap(),
                        world: "greentic:component/telegram@0.2.0".into(),
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
        serde_json::to_string_pretty(&provider_describe).unwrap(),
    )
    .unwrap();
}

pub fn setup_fixture_extensions(home: &Path) {
    let extensions_root = home.join("extensions");
    std::fs::create_dir_all(&extensions_root).unwrap();
    write_design_fixture(&extensions_root);
    write_provider_fixture(&extensions_root);
}
