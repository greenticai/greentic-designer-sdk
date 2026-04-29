use std::path::{Path, PathBuf};

use anyhow::Result;
use greentic_extension_sdk_contract::{CapabilityRef, DescribeJson, ExtensionKind};
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
            })
            .collect();
        let required: Vec<CapabilityRef> = self
            .required
            .into_iter()
            .map(|(id, v)| CapabilityRef {
                id: id.parse().expect("valid cap id"),
                version: v,
            })
            .collect();

        let describe = DescribeJson {
            schema_ref: None,
            api_version: "greentic.ai/v1".into(),
            kind: self.kind,
            metadata: greentic_extension_sdk_contract::describe::Metadata {
                id: self.id.clone(),
                name: self.id.clone(),
                version: self.version.clone(),
                summary: "test".into(),
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
            capabilities: greentic_extension_sdk_contract::describe::Capabilities { offered, required },
            runtime: greentic_extension_sdk_contract::describe::Runtime {
                component: "extension.wasm".into(),
                memory_limit_mb: 64,
                permissions: greentic_extension_sdk_contract::describe::Permissions::default(),
                gtpack: None,
            },
            execution: None,
            contributions: serde_json::json!({}),
            signature: None,
        };
        let describe_path = dir.path().join("describe.json");
        std::fs::write(&describe_path, serde_json::to_vec_pretty(&describe)?)?;
        std::fs::write(dir.path().join("extension.wasm"), &self.wasm_bytes)?;
        Ok(ExtensionFixture { dir, describe_path })
    }
}
