use std::path::Path;

use clap::{Args as ClapArgs, ValueEnum};
use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::storage::Storage;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum KindArg {
    #[value(name = "design")]
    Design,
    #[value(name = "bundle")]
    Bundle,
    #[value(name = "deploy")]
    Deploy,
    #[value(name = "provider")]
    Provider,
    #[value(name = "mcp")]
    Mcp,
    #[value(name = "addon")]
    Addon,
    #[value(name = "all")]
    All,
}

impl KindArg {
    fn to_extension_kind(self) -> Option<ExtensionKind> {
        match self {
            KindArg::Design => Some(ExtensionKind::Design),
            KindArg::Bundle => Some(ExtensionKind::Bundle),
            KindArg::Deploy => Some(ExtensionKind::Deploy),
            KindArg::Provider => Some(ExtensionKind::Provider),
            KindArg::Mcp => Some(ExtensionKind::WasixMcpRouter),
            KindArg::Addon => Some(ExtensionKind::Addon),
            KindArg::All => None,
        }
    }
}

/// Expand the `--kind` argument into the set of kinds to sweep.
///
/// `All` derives from `ExtensionKind::ALL`; it used to be a hand-written vec,
/// which is the same pattern that left `gtdx search` unable to see providers.
fn kinds_for(arg: KindArg) -> Vec<ExtensionKind> {
    arg.to_extension_kind()
        .map_or_else(|| ExtensionKind::ALL.to_vec(), |kind| vec![kind])
}

#[derive(ClapArgs, Debug, Copy, Clone)]
pub struct Args {
    #[arg(long, value_enum, default_value_t = KindArg::All)]
    pub kind: KindArg,
    /// Show enabled/disabled status column.
    #[arg(long)]
    pub status: bool,
}

pub fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    let storage = Storage::new(home);

    let kinds: Vec<ExtensionKind> = kinds_for(args.kind);

    let state = if args.status {
        Some(greentic_extension_sdk_state::ExtensionState::load(home).unwrap_or_default())
    } else {
        None
    };

    for kind in kinds {
        let dir = storage.kind_dir(kind);
        if !dir.exists() {
            continue;
        }
        let mut any = false;
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let describe_path = entry.path().join("describe.json");
            if !describe_path.exists() {
                continue;
            }
            let bytes = std::fs::read(&describe_path)?;
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            let d: greentic_extension_sdk_contract::DescribeJson = serde_json::from_value(value)?;
            if !any {
                println!("[{}]", kind.dir_name());
                any = true;
            }
            if let Some(state) = state.as_ref() {
                let status_label = if state.is_enabled(&d.metadata.id, &d.metadata.version) {
                    "enabled"
                } else {
                    "disabled"
                };
                println!(
                    "  {:<40} {:<12} {:<10} {}",
                    d.metadata.id,
                    d.metadata.version,
                    status_label,
                    d.metadata.summary.default()
                );
            } else {
                println!(
                    "  {}@{}  {}",
                    d.metadata.id,
                    d.metadata.version,
                    d.metadata.summary.default()
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::KindArg;
    use clap::ValueEnum;
    use greentic_extension_sdk_contract::ExtensionKind;

    /// `KindArg` must stay hand-written because clap needs literal variants,
    /// so it cannot derive from `ExtensionKind::ALL`. This test is the
    /// substitute: every kind must be reachable from the CLI, or a kind
    /// exists that `gtdx list --kind` cannot name.
    #[test]
    fn kind_arg_covers_every_extension_kind() {
        // Kinds that are never INSTALLED under `~/.greentic/extensions/<dir>/`,
        // so `gtdx list --kind <it>` could only ever print nothing. Offering
        // the flag would advertise a lookup that cannot succeed.
        const NOT_INSTALLED_AS_EXTENSIONS: &[ExtensionKind] = &[ExtensionKind::AgenticWorker];

        let reachable: Vec<ExtensionKind> = KindArg::value_variants()
            .iter()
            .filter_map(|k| k.to_extension_kind())
            .collect();

        for kind in ExtensionKind::ALL.iter().copied() {
            if NOT_INSTALLED_AS_EXTENSIONS.contains(&kind) {
                continue;
            }
            assert!(
                reachable.contains(&kind),
                "no KindArg variant maps to {kind:?} — add one, with \
                 #[value(name = \"{}\")]",
                kind.dir_name()
            );
        }
    }

    /// The `--kind all` branch must sweep every kind, not a frozen list.
    #[test]
    fn all_expands_to_every_kind() {
        assert_eq!(super::kinds_for(KindArg::All), ExtensionKind::ALL.to_vec());
    }

    /// `kind_arg_covers_every_extension_kind` only asserts every
    /// `ExtensionKind` is *reachable* from some `KindArg`; it never checks
    /// that the clap flag value (`#[value(name = "...")]`) equals the
    /// on-disk directory name it claims to match. Renaming a `dir_name`
    /// compiles and passes every other test while silently splitting the
    /// CLI flag from the install directory it is supposed to select.
    #[test]
    fn kind_arg_value_name_matches_dir_name() {
        for arg in KindArg::value_variants() {
            let Some(kind) = arg.to_extension_kind() else {
                continue; // `all` has no single dir_name to match.
            };
            let value_name = arg
                .to_possible_value()
                .expect("every non-skipped KindArg variant has a possible value")
                .get_name()
                .to_string();
            assert_eq!(
                value_name,
                kind.dir_name(),
                "KindArg::{arg:?}'s #[value(name = \"{value_name}\")] must equal \
                 ExtensionKind::{kind:?}::dir_name() (\"{}\")",
                kind.dir_name(),
            );
        }
    }

    #[test]
    fn a_specific_kind_expands_to_just_that_kind() {
        assert_eq!(
            super::kinds_for(KindArg::Provider),
            vec![ExtensionKind::Provider]
        );
    }
}
