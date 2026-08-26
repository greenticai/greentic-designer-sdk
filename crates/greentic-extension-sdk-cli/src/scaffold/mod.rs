//! Scaffolding logic for `gtdx new`.

pub mod contract_lock;
pub mod embedded;
pub mod openapi;
pub mod preflight;
pub mod template;

/// Extension kinds that can be scaffolded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    Design,
    Bundle,
    Deploy,
    Provider,
    WasmComponent,
    Mcp,
    Llm,
    Addon,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Design => "design",
            Kind::Bundle => "bundle",
            Kind::Deploy => "deploy",
            Kind::Provider => "provider",
            Kind::WasmComponent => "wasm-component",
            Kind::Mcp => "mcp",
            Kind::Llm => "llm",
            Kind::Addon => "addon",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_component_kind_str() {
        assert_eq!(Kind::WasmComponent.as_str(), "wasm-component");
    }

    #[test]
    fn mcp_kind_str() {
        assert_eq!(Kind::Mcp.as_str(), "mcp");
    }

    #[test]
    fn llm_kind_str() {
        assert_eq!(Kind::Llm.as_str(), "llm");
    }

    /// A new `ExtensionKind` compiles and passes every kind-dependent
    /// guard in the contract and CLI crates (install, list, search,
    /// validate) without a matching `scaffold::Kind` variant, template
    /// tree, embedded WIT, or wizard entry — `gtdx new --kind <it>` would
    /// simply reject it as an unknown value, with no test saying so.
    ///
    /// This ties the two enums together: every `ExtensionKind::ALL` entry
    /// must be scaffoldable via some `scaffold::Kind` (matched by
    /// `dir_name()` == `as_str()`), unless explicitly exempted below with a
    /// comment explaining why. There are no exemptions today — every
    /// `ExtensionKind` `gtdx new` knows about can be scaffolded from
    /// scratch.
    #[test]
    fn every_extension_kind_is_scaffoldable() {
        use clap::ValueEnum as _;
        use greentic_extension_sdk_contract::ExtensionKind;

        // Deliberately non-scaffoldable `ExtensionKind`s go here, with a
        // comment saying why `gtdx new` cannot produce them.
        const NON_SCAFFOLDABLE: &[ExtensionKind] = &[];

        let scaffoldable: Vec<&'static str> =
            Kind::value_variants().iter().map(|k| k.as_str()).collect();

        for kind in ExtensionKind::ALL.iter().copied() {
            if NON_SCAFFOLDABLE.contains(&kind) {
                continue;
            }
            assert!(
                scaffoldable.contains(&kind.dir_name()),
                "ExtensionKind::{kind:?} (dir_name {:?}) has no matching scaffold::Kind — \
                 it can install/list/search/validate but `gtdx new --kind <it>` would reject it",
                kind.dir_name(),
            );
        }
    }
}
