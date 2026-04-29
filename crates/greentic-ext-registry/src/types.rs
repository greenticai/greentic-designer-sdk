use greentic_ext_contract::{DescribeJson, ExtensionKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub kind: Option<ExtensionKind>,
    pub capability: Option<String>,
    pub query: Option<String>,
    pub page: u32,
    pub limit: u32,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            kind: None,
            capability: None,
            query: None,
            page: 1,
            limit: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionSummary {
    pub name: String,
    pub latest_version: String,
    pub kind: ExtensionKind,
    pub summary: String,
    #[serde(default)]
    pub downloads: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionMetadata {
    pub name: String,
    pub version: String,
    pub describe: DescribeJson,
    pub artifact_sha256: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub yanked: bool,
}

pub type ArtifactBytes = Vec<u8>;

#[derive(Debug, Clone)]
pub struct ExtensionArtifact {
    pub name: String,
    pub version: String,
    pub describe: DescribeJson,
    pub bytes: ArtifactBytes,
    pub signature: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuthToken {
    pub registry: String,
    pub token: String,
}
