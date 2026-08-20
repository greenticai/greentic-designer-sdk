use std::io::IsTerminal;
use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::storage::Storage;

use super::{ALL_KINDS, split_name_version};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Extension id to remove (all installed versions unless --version is given)
    pub name: String,

    /// Remove only this exact version
    #[arg(short = 'v', long)]
    pub version: Option<String>,

    /// Skip the confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Show what would be removed without deleting anything
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(args: &Args, home: &Path) -> anyhow::Result<()> {
    let storage = Storage::new(home);

    // Collect first, then act: `uninstall` is a recursive delete, and it used
    // to run one directory at a time with no prompt and no summary.
    let mut matches = Vec::new();
    for kind in ALL_KINDS {
        let dir = storage.kind_dir(kind);
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let dname = entry.file_name();
            let dname_str = dname.to_string_lossy();
            let Some((name, version)) = split_name_version(&dname_str) else {
                continue;
            };
            if name != args.name {
                continue;
            }
            if let Some(want) = &args.version
                && want != version
            {
                continue;
            }
            matches.push((name.to_string(), version.to_string(), entry.path()));
        }
    }

    if matches.is_empty() {
        // Previously printed to stderr and returned Ok(()), so a typo'd name
        // looked like success to any script checking the exit status.
        anyhow::bail!(
            "no installed extension matches {:?}{}; list what is installed with: gtdx list",
            args.name,
            args.version
                .as_ref()
                .map_or(String::new(), |v| format!(" at version {v}"))
        );
    }

    for (name, version, path) in &matches {
        println!("  {name}@{version}  ({})", path.display());
    }
    if args.dry_run {
        println!("dry-run: nothing was removed");
        return Ok(());
    }
    confirm_removal(matches.len(), args.yes)?;

    for (name, version, path) in &matches {
        std::fs::remove_dir_all(path)
            .map_err(|e| anyhow::anyhow!("remove {}: {e}", path.display()))?;
        println!("✓ removed {name}@{version}");
    }
    Ok(())
}

fn confirm_removal(count: usize, assume_yes: bool) -> anyhow::Result<()> {
    if assume_yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "refusing to remove {count} extension(s) without confirmation; \
             re-run with --yes, or --dry-run to preview"
        );
    }
    let ok = dialoguer::Confirm::new()
        .with_prompt(format!("Remove {count} extension(s)?"))
        .default(false)
        .interact()
        .unwrap_or(false);
    if !ok {
        anyhow::bail!("cancelled: nothing was removed");
    }
    Ok(())
}
