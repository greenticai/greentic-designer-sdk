use greentic_extension_sdk_contract::{DescribeJson, ExtensionKind};
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

#[derive(Clone)]
pub struct AuthToken {
    pub registry: String,
    pub token: String,
}

// Hand-written so the secret `token` never leaks via `{:?}`/tracing (audit N10).
impl std::fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthToken")
            .field("registry", &self.registry)
            .field("token", &"***")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::AuthToken;

    #[test]
    fn debug_redacts_token() {
        let t = AuthToken {
            registry: "store.greentic.cloud".into(),
            token: "super-secret-token".into(),
        };
        let rendered = format!("{t:?}");
        assert!(
            !rendered.contains("super-secret-token"),
            "token leaked into Debug: {rendered}"
        );
        assert!(rendered.contains("store.greentic.cloud"));
    }
}
