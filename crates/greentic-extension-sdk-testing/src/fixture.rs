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

        let offered = parse_capability_refs(self.offered)?;
        let required = parse_capability_refs(self.required)?;

        // The single `stub` component references the on-disk `extension.wasm`
        // via a gtpack with its real sha256, so the describe and the payload are
        // coherent: a source-dir loader can find the wasm, and the default
        // (no `.with_wasm`) path is just as valid as the explicit one — the
        // previous oci_ref-only stub left `extension.wasm` unreferenced and made
        // the default path internally inconsistent (audit P1-10 / upstream #8).
        let wasm_sha = greentic_extension_sdk_contract::artifact_sha256(&self.wasm_bytes);
        let stub_components = {
            let id = "stub"
                .parse::<greentic_extension_sdk_contract::ComponentId>()
                .map_err(|e| anyhow::anyhow!("stub component id: {e}"))?;
            let sha256 = wasm_sha
                .parse::<greentic_extension_sdk_contract::Sha256>()
                .map_err(|e| anyhow::anyhow!("stub sha256: {e}"))?;
            let mut m = std::collections::BTreeMap::new();
            m.insert(
                id,
                greentic_extension_sdk_contract::RuntimeComponent {
                    oci_ref: None,
                    gtpack: Some(greentic_extension_sdk_contract::RuntimeGtpack {
                        file: "extension.wasm".into(),
                        sha256: wasm_sha.clone(),
                        pack_id: "stub".into(),
                        component_version: "0.0.0".into(),
                    }),
                    sha256,
                    world: "greentic:component/stub@0.1.0".into(),
                },
            );
            m
        };

        let mut describe = DescribeJson {
            secret_requirements: Vec::new(),
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
            engine: Some(greentic_extension_sdk_contract::describe::Engine {
                greentic_designer: "*".into(),
                ext_runtime: "*".into(),
            }),
            capabilities: greentic_extension_sdk_contract::describe::Capabilities {
                offered,
                required,
            },
            runtime: greentic_extension_sdk_contract::describe::Runtime {
                world: None,
                memory_limit_mb: 64,
                permissions: greentic_extension_sdk_contract::describe::Permissions::default(),
                components: stub_components,
            },
            execution: None,
            contributions: greentic_extension_sdk_contract::describe::Contributions::default(),
            localization: None,
            signature: None,
            manifest_sha256: None,
            required_secrets: vec![],
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

/// Convert caller-supplied `(id, version)` pairs into `CapabilityRef`s,
/// surfacing a malformed capability id as `Err` (with the offending id named)
/// rather than panicking — `testing` is a published crate and `build()` is
/// fallible (audit cycle-2 N2).
fn parse_capability_refs(pairs: Vec<(String, String)>) -> Result<Vec<CapabilityRef>> {
    pairs
        .into_iter()
        .map(|(id, version)| {
            let id = id
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid capability id {id:?}: {e}"))?;
            Ok(CapabilityRef {
                id,
                version,
                deprecated: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `build()` returns `Result`, so a caller-supplied invalid capability id
    /// must surface as `Err`, never an internal panic. `testing` is a published
    /// crate; an `.expect()` here would crash external users' test suites with
    /// an opaque message (audit cycle-2 N2).
    #[test]
    fn build_returns_err_on_invalid_offered_capability_id() {
        let result = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.x", "1.0.0")
            // No `namespace:name` colon → MalformedCapabilityId.
            .offer("not-a-valid-cap-id", "1.0.0")
            .build();
        let Err(err) = result else {
            panic!("invalid capability id must be an Err, not a panic")
        };
        assert!(
            err.to_string().contains("not-a-valid-cap-id"),
            "error should name the offending id; got: {err}"
        );
    }

    #[test]
    fn build_returns_err_on_invalid_required_capability_id() {
        let result = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.x", "1.0.0")
            .require("also-invalid", "1.0.0")
            .build();
        assert!(
            result.is_err(),
            "invalid required capability id must be an Err, not a panic"
        );
    }

    #[test]
    fn default_build_is_coherent_component_references_on_disk_wasm() {
        // Even without `.with_wasm`, the describe's component must reference the
        // on-disk extension.wasm via gtpack with its real sha256 (audit P1-10).
        let fx = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.x", "1.0.0")
            .build()
            .expect("default build should succeed");
        let bytes = std::fs::read(&fx.describe_path).unwrap();
        let describe: greentic_extension_sdk_contract::DescribeJson =
            serde_json::from_slice(&bytes).expect("describe.json must deserialize");
        let comp = describe.runtime.components.values().next().unwrap();
        let gtpack = comp
            .gtpack
            .as_ref()
            .expect("stub must reference a gtpack, not a bare oci_ref");
        assert_eq!(gtpack.file, "extension.wasm");
        let on_disk = std::fs::read(fx.root().join("extension.wasm")).unwrap();
        assert_eq!(
            gtpack.sha256,
            greentic_extension_sdk_contract::artifact_sha256(&on_disk),
            "declared gtpack sha256 must match the on-disk extension.wasm"
        );
    }
}
