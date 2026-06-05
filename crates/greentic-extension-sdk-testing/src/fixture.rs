use std::path::{Path, PathBuf};

use anyhow::Result;
use greentic_extension_sdk_contract::{
    CapabilityRef, DescribeJson, ExtensionKind, LocalizedString, bind_manifest, build_manifest,
};
use tempfile::TempDir;

pub struct ExtensionFixture {
    pub dir: TempDir,
    pub describe_path: PathBuf,
}

impl ExtensionFixture {
    #[must_use]
    pub fn root(&self) -> &Path {
        self.dir.path()
    }
}

pub struct ExtensionFixtureBuilder {
    kind: ExtensionKind,
    id: String,
    version: String,
    offered: Vec<(String, String)>,
    required: Vec<(String, String)>,
    wasm_bytes: Vec<u8>,
}

impl ExtensionFixtureBuilder {
    #[must_use]
    pub fn new(kind: ExtensionKind, id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            version: version.into(),
            offered: Vec::new(),
            required: Vec::new(),
            wasm_bytes: Vec::new(),
        }
    }

    #[must_use]
    pub fn offer(mut self, id: impl Into<String>, v: impl Into<String>) -> Self {
        self.offered.push((id.into(), v.into()));
        self
    }

    #[must_use]
    pub fn require(mut self, id: impl Into<String>, v: impl Into<String>) -> Self {
        self.required.push((id.into(), v.into()));
        self
    }

    #[must_use]
    pub fn with_wasm(mut self, bytes: Vec<u8>) -> Self {
        self.wasm_bytes = bytes;
        self
    }

    pub fn build(self) -> Result<ExtensionFixture> {
        let dir = TempDir::new()?;

        let offered: Vec<CapabilityRef> = self
            .offered
            .into_iter()
            .map(|(id, v)| CapabilityRef {
                id: id.parse().expect("valid cap id"),
                version: v,
                deprecated: None,
            })
            .collect();
        let required: Vec<CapabilityRef> = self
            .required
            .into_iter()
            .map(|(id, v)| CapabilityRef {
                id: id.parse().expect("valid cap id"),
                version: v,
                deprecated: None,
            })
            .collect();

        let mut describe = DescribeJson {
            schema_ref: None,
            api_version: "greentic.ai/v2".into(),
            kind: self.kind,
            compat: greentic_extension_sdk_contract::Compat {
                min_designer_version: ">=1.0.0".parse().unwrap(),
                min_runner_version: "^0.12.0".parse().unwrap(),
                contract_version: "1.2.0".parse().unwrap(),
            },
            metadata: greentic_extension_sdk_contract::describe::Metadata {
                id: self.id.clone(),
                name: self.id.clone(),
                version: self.version.clone(),
                summary: LocalizedString::plain("test"),
                description: None,
                author: greentic_extension_sdk_contract::describe::Author {
                    name: "test".into(),
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
            engine: greentic_extension_sdk_contract::describe::Engine {
                greentic_designer: "*".into(),
                ext_runtime: "*".into(),
            },
            capabilities: greentic_extension_sdk_contract::describe::Capabilities {
                offered,
                required,
            },
            runtime: greentic_extension_sdk_contract::describe::Runtime {
                memory_limit_mb: 64,
                permissions: greentic_extension_sdk_contract::describe::Permissions::default(),
                components: {
                    let mut m = std::collections::BTreeMap::new();
                    m.insert(
                        "stub"
                            .parse::<greentic_extension_sdk_contract::ComponentId>()
                            .expect("valid component id"),
                        greentic_extension_sdk_contract::RuntimeComponent {
                            oci_ref: Some("oci://ghcr.io/example/stub:latest".into()),
                            gtpack: None,
                            sha256:
                                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                                    .parse()
                                    .expect("valid sha256"),
                            world: "greentic:component/stub@0.1.0".into(),
                        },
                    );
                    m
                },
            },
            execution: None,
            contributions: greentic_extension_sdk_contract::describe::Contributions::default(),
            localization: None,
            signature: None,
            manifest_sha256: None,
        };
        // Write the WASM component first, then build a whole-archive manifest
        // over the on-disk entries and bind it into the describe BEFORE the
        // describe is serialized. The install path enforces this manifest
        // integrity ledger unconditionally (audit P0-3), so a fixture without
        // a bound manifest.json would fail to install. `build_manifest` excludes
        // describe.json and manifest.json itself, so the manifest covers only
        // `extension.wasm` here — and binding it does not depend on the (not yet
        // serialized) describe.json bytes.
        std::fs::write(dir.path().join("extension.wasm"), &self.wasm_bytes)?;
        let manifest = build_manifest(vec![("extension.wasm", self.wasm_bytes.as_slice())]);
        let manifest_json = serde_json::to_vec(&manifest)?;
        bind_manifest(&mut describe, &manifest_json);

        let describe_path = dir.path().join("describe.json");
        std::fs::write(&describe_path, serde_json::to_vec_pretty(&describe)?)?;
        std::fs::write(
            dir.path()
                .join(greentic_extension_sdk_contract::MANIFEST_ENTRY_NAME),
            &manifest_json,
        )?;
        Ok(ExtensionFixture { dir, describe_path })
    }
}
