use std::path::{Path, PathBuf};

use async_trait::async_trait;
use greentic_ext_contract::DescribeJson;
use serde_json::Value;

use crate::error::RegistryError;
use crate::registry::ExtensionRegistry;
use crate::types::{
    ArtifactBytes, ExtensionArtifact, ExtensionMetadata, ExtensionSummary, SearchQuery,
};

pub struct LocalFilesystemRegistry {
    name: String,
    root: PathBuf,
}

impl LocalFilesystemRegistry {
    pub fn new(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
        }
    }

    fn parse_pack_filename(filename: &str) -> Option<(String, String)> {
        let stem = filename.strip_suffix(".gtxpack")?;
        let idx = stem.rfind('-')?;
        let (name, version) = stem.split_at(idx);
        let version = version.strip_prefix('-')?.to_string();
        if !name.is_empty() && !version.is_empty() {
            Some((name.to_string(), version))
        } else {
            None
        }
    }

    fn resolve_pack_path(&self, name: &str, version: &str) -> Option<PathBuf> {
        // Hierarchical preferred (Track C publish): <root>/<key>/<version>/<name>-<version>.gtxpack
        // where `key` may be either id or name.
        let hierarchical = self
            .root
            .join(name)
            .join(version)
            .join(format!("{name}-{version}.gtxpack"));
        if hierarchical.is_file() {
            return Some(hierarchical);
        }
        for entry in std::fs::read_dir(&self.root).ok()?.flatten() {
            if !entry.file_type().ok()?.is_dir() {
                continue;
            }
            let ext_dir = entry.path();
            let ver_dir = ext_dir.join(version);
            if !ver_dir.is_dir() {
                continue;
            }
            for pack in std::fs::read_dir(&ver_dir).ok()?.flatten() {
                let path = pack.path();
                if path.extension().and_then(|s| s.to_str()) == Some("gtxpack") {
                    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if let Some((n, v)) = Self::parse_pack_filename(filename)
                        && v == version
                        && (n == name
                            || ext_dir
                                .file_name()
                                .and_then(|s| s.to_str())
                                .is_some_and(|id| id == name))
                    {
                        return Some(path);
                    }
                }
            }
        }
        // Fallback: flat layout (dev-local scratch dir).
        let flat = self.root.join(format!("{name}-{version}.gtxpack"));
        if flat.is_file() {
            return Some(flat);
        }
        None
    }

    fn pack_path(&self, name: &str, version: &str) -> PathBuf {
        self.resolve_pack_path(name, version)
            .unwrap_or_else(|| self.root.join(format!("{name}-{version}.gtxpack")))
    }

    /// Return the on-disk root path of this registry.
    #[must_use]
    pub fn root_path(&self) -> &Path {
        &self.root
    }

    fn read_describe_from_pack(pack_path: &Path) -> Result<DescribeJson, RegistryError> {
        let file = std::fs::File::open(pack_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| RegistryError::Storage(format!("zip open: {e}")))?;
        let mut describe_entry = archive
            .by_name("describe.json")
            .map_err(|e| RegistryError::Storage(format!("describe.json missing: {e}")))?;
        let value: Value = serde_json::from_reader(&mut describe_entry)?;
        greentic_ext_contract::schema::validate_describe_json(&value)?;
        let describe: DescribeJson = serde_json::from_value(value)?;
        Ok(describe)
    }

    fn read_artifact_bytes(pack_path: &Path) -> Result<ArtifactBytes, RegistryError> {
        Ok(std::fs::read(pack_path)?)
    }

    fn list_packs(&self) -> std::io::Result<Vec<(String, String, PathBuf)>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let path = entry.path();
            if ft.is_file() {
                // Flat: <root>/<name>-<version>.gtxpack
                let filename = entry.file_name();
                let filename_str = filename.to_string_lossy();
                if let Some((n, v)) = Self::parse_pack_filename(&filename_str) {
                    out.push((n, v, path));
                }
            } else if ft.is_dir() {
                // Hierarchical: <root>/<id>/<version>/<name>-<version>.gtxpack
                for ver_entry in std::fs::read_dir(&path)?.flatten() {
                    if !ver_entry.file_type()?.is_dir() {
                        continue;
                    }
                    for pack_entry in std::fs::read_dir(ver_entry.path())?.flatten() {
                        let pack_path = pack_entry.path();
                        if pack_path.extension().and_then(|s| s.to_str()) != Some("gtxpack") {
                            continue;
                        }
                        let filename = pack_entry.file_name();
                        let filename_str = filename.to_string_lossy();
                        if let Some((n, v)) = Self::parse_pack_filename(&filename_str) {
                            out.push((n, v, pack_path));
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl ExtensionRegistry for LocalFilesystemRegistry {
    fn name(&self) -> &str {
        &self.name
    }

    async fn search(&self, query: SearchQuery) -> Result<Vec<ExtensionSummary>, RegistryError> {
        let mut summaries = Vec::new();
        for (name, version, path) in self.list_packs()? {
            if let Some(q) = &query.query
                && !name.contains(q.as_str())
            {
                continue;
            }
            match Self::read_describe_from_pack(&path) {
                Ok(d) => {
                    if let Some(k) = query.kind
                        && d.kind != k
                    {
                        continue;
                    }
                    summaries.push(ExtensionSummary {
                        name: d.metadata.id,
                        latest_version: version,
                        kind: d.kind,
                        summary: d.metadata.summary,
                        downloads: 0,
                    });
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping invalid pack");
                }
            }
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(summaries.into_iter().take(query.limit as usize).collect())
    }

    async fn metadata(
        &self,
        name: &str,
        version: &str,
    ) -> Result<ExtensionMetadata, RegistryError> {
        let path = self.pack_path(name, version);
        if !path.exists() {
            return Err(RegistryError::NotFound {
                name: name.into(),
                version: version.into(),
            });
        }
        let describe = Self::read_describe_from_pack(&path)?;
        let bytes = Self::read_artifact_bytes(&path)?;
        let sha = greentic_ext_contract::artifact_sha256(&bytes);
        Ok(ExtensionMetadata {
            name: describe.metadata.id.clone(),
            version: describe.metadata.version.clone(),
            describe,
            artifact_sha256: sha,
            published_at: String::new(),
            yanked: false,
        })
    }

    async fn fetch(&self, name: &str, version: &str) -> Result<ExtensionArtifact, RegistryError> {
        let path = self.pack_path(name, version);
        if !path.exists() {
            return Err(RegistryError::NotFound {
                name: name.into(),
                version: version.into(),
            });
        }
        let describe = Self::read_describe_from_pack(&path)?;
        let bytes = Self::read_artifact_bytes(&path)?;
        Ok(ExtensionArtifact {
            name: describe.metadata.id.clone(),
            version: describe.metadata.version.clone(),
            describe,
            bytes,
            signature: None,
        })
    }

    async fn list_versions(&self, name: &str) -> Result<Vec<String>, RegistryError> {
        let mut versions: Vec<String> = self
            .list_packs()?
            .into_iter()
            .filter(|(n, _, _)| n == name)
            .map(|(_, v, _)| v)
            .collect();
        versions.sort();
        Ok(versions)
    }

    async fn publish(
        &self,
        req: crate::publish::PublishRequest,
    ) -> Result<crate::publish::PublishReceipt, RegistryError> {
        self.publish_local(&req)
    }
}
