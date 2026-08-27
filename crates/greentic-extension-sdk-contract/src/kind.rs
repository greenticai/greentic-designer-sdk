use serde::{Deserialize, Serialize};

/// `#[non_exhaustive]` so adding a future kind (kind #7 and beyond) is
/// additive: from that point on, an exhaustive `match` outside this crate is
/// forced to carry a wildcard arm, so it keeps compiling instead of breaking
/// on every new variant after this one. This release is not itself covered
/// by that guarantee — adding both `#[non_exhaustive]` and `Addon` in the
/// same release still breaks any downstream exhaustive `match` written
/// against the previous, five-variant enum, on both counts at once. Source
/// compatibility starts from the *next* variant added after this one.
/// Exhaustive matching over every variant still works *inside* this crate
/// (see `dir_name`/`wire_name` below and `tests::all_covers_every_variant`),
/// since `non_exhaustive` only restricts construction and matching from
/// outside the defining crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
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
    /// A component the platform provisions and reconciles as declarative
    /// infrastructure — a Qdrant, a Redis — rather than a flow-time or
    /// design-time extension. It implements `greentic:extension-addon@0.1.0`'s
    /// `addon-extension` world (`validation` + `workload` + `reconciler`); see
    /// `wit/extension-addon.wit`.
    #[serde(rename = "AddonExtension")]
    Addon,
}

impl ExtensionKind {
    /// Every variant, so callers that must sweep all install directories can
    /// iterate this instead of hand-listing variants. A hand-written list
    /// silently goes stale when a kind is added — `gtdx uninstall` shipped one
    /// that omitted `Provider`, so provider extensions could not be removed at
    /// all while the command still reported success.
    ///
    /// A slice, not an array: an array's length (`[Self; N]`) is part of its
    /// type, so adding a variant here used to change every call site's
    /// inferred type right along with the const. A slice's length is a
    /// runtime property, not a type parameter, so a new variant changes only
    /// this initializer.
    pub const ALL: &'static [Self] = &[
        Self::Design,
        Self::Bundle,
        Self::Deploy,
        Self::Provider,
        Self::WasixMcpRouter,
        Self::Addon,
    ];

    /// A variant added without a matching entry in `ALL` compiles fine —
    /// nothing here forces `ALL` to be exhaustive; the compiler only forces
    /// the match arms in `tests::all_covers_every_variant` (and `dir_name`
    /// and `wire_name` below) to name every variant, because their scrutinee
    /// is typed as `Self`. This assertion is the actual guard: it fails the
    /// build the moment `ALL`'s length stops matching the variant count,
    /// which is exactly the drift `gtdx uninstall` shipped with once
    /// (omitted `Provider`, so provider extensions could not be removed at
    /// all while the command still reported success). Bump this alongside
    /// adding a variant to `ALL`.
    const _ASSERT_ALL_COVERS_EVERY_VARIANT: () = assert!(Self::ALL.len() == 6);

    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Bundle => "bundle",
            Self::Deploy => "deploy",
            Self::Provider => "provider",
            Self::WasixMcpRouter => "mcp",
            Self::Addon => "addon",
        }
    }

    /// The `serde` wire value for this kind, as it appears in
    /// `describe.json`'s `kind` field.
    ///
    /// Declared separately from the `#[serde(rename = "…")]` attributes
    /// because attributes are not readable at runtime. `wire_name_matches_serde`
    /// in `tests/kind.rs` asserts the two agree, so the duplication cannot
    /// drift silently.
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Design => "DesignExtension",
            Self::Bundle => "BundleExtension",
            Self::Deploy => "DeployExtension",
            Self::Provider => "ProviderExtension",
            Self::WasixMcpRouter => "wasix:mcp/router",
            Self::Addon => "AddonExtension",
        }
    }

    /// Inverse of [`Self::wire_name`]. `None` for anything this contract
    /// version does not know — callers decide whether that is an error or a
    /// skip.
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.wire_name() == s)
    }

    /// Inverse of [`Self::dir_name`].
    #[must_use]
    pub fn from_dir_name(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.dir_name() == s)
    }
}

#[cfg(test)]
mod tests {
    use super::ExtensionKind;

    /// The match below is exhaustive over `ExtensionKind`, so adding a
    /// variant forces a new arm here — but that only proves the arm list is
    /// complete, not that `ALL` is: the scrutinee is `ALL.iter()`, so a
    /// variant missing from `ALL` itself would simply never reach this match
    /// and everything would keep compiling and passing. The actual guard for
    /// that is `_ASSERT_ALL_COVERS_EVERY_VARIANT`'s `assert!` next to `ALL`'s
    /// definition, which pins `ALL.len()` to the variant count at compile
    /// time.
    #[test]
    fn all_covers_every_variant() {
        for kind in ExtensionKind::ALL.iter().copied() {
            match kind {
                ExtensionKind::Design
                | ExtensionKind::Bundle
                | ExtensionKind::Deploy
                | ExtensionKind::Provider
                | ExtensionKind::WasixMcpRouter
                | ExtensionKind::Addon => {}
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
