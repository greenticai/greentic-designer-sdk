use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExtensionKind {
    #[serde(rename = "DesignExtension")]
    Design,
    #[serde(rename = "BundleExtension")]
    Bundle,
    #[serde(rename = "DeployExtension")]
    Deploy,
    #[serde(rename = "ProviderExtension")]
    Provider,
}

impl ExtensionKind {
    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Bundle => "bundle",
            Self::Deploy => "deploy",
            Self::Provider => "provider",
        }
    }
}
