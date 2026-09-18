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
    /// Every variant, so callers that must sweep all install directories can
    /// iterate this instead of hand-listing variants. A hand-written list
    /// silently goes stale when a kind is added — `gtdx uninstall` shipped one
    /// that omitted `Provider`, so provider extensions could not be removed at
    /// all while the command still reported success.
    pub const ALL: [Self; 5] = [
        Self::Design,
        Self::Bundle,
        Self::Deploy,
        Self::Provider,
        Self::WasixMcpRouter,
    ];

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

#[cfg(test)]
mod tests {
    use super::ExtensionKind;

    /// `ALL` must stay exhaustive. If a variant is added, the match below stops
    /// compiling — which is the point: the compiler, not review, catches drift.
    #[test]
    fn all_covers_every_variant() {
        for kind in ExtensionKind::ALL {
            match kind {
                ExtensionKind::Design
                | ExtensionKind::Bundle
                | ExtensionKind::Deploy
                | ExtensionKind::Provider
                | ExtensionKind::WasixMcpRouter => {}
            }
        }
        let mut dirs: Vec<_> = ExtensionKind::ALL.iter().map(|k| k.dir_name()).collect();
        dirs.sort_unstable();
        dirs.dedup();
        assert_eq!(
            dirs.len(),
            ExtensionKind::ALL.len(),
            "dir_name must be unique per kind"
        );
    }
}
