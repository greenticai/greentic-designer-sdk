use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::storage::Storage;

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub name: String,
    #[arg(long)]
    pub version: Option<String>,
}

pub fn run(args: &Args, home: &Path) -> anyhow::Result<()> {
    let storage = Storage::new(home);
    let mut removed_any = false;
    // Every kind, not a hand-written subset: this list used to omit
    // `Provider`, so `gtdx uninstall` could never remove a provider extension
    // — and said "nothing to remove" while the extension stayed installed.
    for kind in ExtensionKind::ALL.iter().copied() {
        let dir = storage.kind_dir(kind);
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let dname = entry.file_name();
            let dname_str = dname.to_string_lossy();
            if let Some((n, v)) = split_name_version(&dname_str)
                && n == args.name
            {
                if let Some(want_v) = &args.version
                    && want_v != v
                {
                    continue;
                }
                std::fs::remove_dir_all(entry.path())?;
                println!("✓ removed {n}@{v}");
                removed_any = true;
            }
        }
    }
    if !removed_any {
        // Exit non-zero. The caller asked for something to be gone; reporting
        // success while it is still installed is the failure mode that hid the
        // missing `Provider` kind above, and it silently breaks any script
        // that uninstalls and then checks the status.
        anyhow::bail!(
            "nothing to remove for {}{}",
            args.name,
            args.version
                .as_deref()
                .map_or(String::new(), |v| format!("@{v}"))
        );
    }
    Ok(())
}

fn split_name_version(dirname: &str) -> Option<(&str, &str)> {
    let idx = dirname.rfind('-')?;
    let (n, rest) = dirname.split_at(idx);
    Some((n, rest.strip_prefix('-')?))
}
