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
    /// A WASM component that exports `wasix:mcp/router` — a flow-capable local
    /// MCP router. Unlike design-extension MCPs (which are agent-only toolsets
    /// that run inside the agentic worker loop), a `wasix:mcp/router` component
    /// is addressable as a flow node and can be loaded by the MCP executor in
    /// the Greentic runner (`greentic-mcp`). Scaffold with `gtdx new --kind mcp`.
    #[serde(rename = "wasix:mcp/router")]
    WasixMcpRouter,
}

impl ExtensionKind {
    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Bundle => "bundle",
            Self::Deploy => "deploy",
            Self::Provider => "provider",
            Self::WasixMcpRouter => "mcp",
        }
    }
}
