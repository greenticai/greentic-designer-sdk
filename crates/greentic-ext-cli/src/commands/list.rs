use std::path::Path;

use clap::{Args as ClapArgs, ValueEnum};
use greentic_ext_contract::ExtensionKind;
use greentic_ext_registry::storage::Storage;

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
            KindArg::All => None,
        }
    }
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

    let kinds: Vec<ExtensionKind> = if let Some(kind) = args.kind.to_extension_kind() {
        vec![kind]
    } else {
        vec![
            ExtensionKind::Design,
            ExtensionKind::Bundle,
            ExtensionKind::Deploy,
            ExtensionKind::Provider,
        ]
    };

    let state = if args.status {
        Some(greentic_ext_state::ExtensionState::load(home).unwrap_or_default())
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
            let d: greentic_ext_contract::DescribeJson = serde_json::from_value(value)?;
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
                    d.metadata.id, d.metadata.version, status_label, d.metadata.summary
                );
            } else {
                println!(
                    "  {}@{}  {}",
                    d.metadata.id, d.metadata.version, d.metadata.summary
                );
            }
        }
    }
    Ok(())
}
