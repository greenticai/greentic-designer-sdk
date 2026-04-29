use std::path::Path;

use anyhow::Context as _;
use clap::Args as ClapArgs;
use greentic_ext_contract::{DescribeJson, ExtensionKind};
use greentic_ext_registry::storage::Storage;

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub name: String,
    #[arg(long)]
    pub version: Option<String>,
    /// Ignored in Wave A (local-first only). Reserved for future remote lookup.
    #[arg(long)]
    pub registry: Option<String>,
}

pub fn run(args: &Args, home: &Path) -> anyhow::Result<()> {
    if args.registry.is_some() {
        tracing::warn!(
            "The --registry flag is ignored in this version; info reads from local installs only."
        );
    }
    let storage = Storage::new(home);
    let found = find_installed(&storage, &args.name, args.version.as_deref())?;
    let (kind, describe) = found.ok_or_else(|| {
        anyhow::anyhow!(
            "extension not installed: {}. Run `gtdx install` first.",
            args.name
        )
    })?;
    render_info(kind, &describe);
    Ok(())
}

fn find_installed(
    storage: &Storage,
    name: &str,
    version: Option<&str>,
) -> anyhow::Result<Option<(ExtensionKind, DescribeJson)>> {
    let all_kinds = [
        ExtensionKind::Design,
        ExtensionKind::Bundle,
        ExtensionKind::Deploy,
        ExtensionKind::Provider,
    ];

    let mut candidates: Vec<(ExtensionKind, semver::Version, DescribeJson)> = Vec::new();

    for kind in all_kinds {
        let kind_dir = storage.kind_dir(kind);
        if !kind_dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&kind_dir)
            .with_context(|| format!("reading extensions dir {}", kind_dir.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let dir_name = entry.file_name();
            let dir_str = dir_name.to_string_lossy();

            // Flat layout: {name}-{version}
            let Some(rest) = dir_str.strip_prefix(name) else {
                continue;
            };
            let Some(ver_str) = rest.strip_prefix('-') else {
                continue;
            };

            // If version filter provided, require exact match.
            if version.is_some_and(|want_ver| ver_str != want_ver) {
                continue;
            }

            let Ok(parsed_ver) = semver::Version::parse(ver_str) else {
                continue;
            };

            let describe_path = entry.path().join("describe.json");
            if !describe_path.exists() {
                continue;
            }
            let bytes = std::fs::read(&describe_path)
                .with_context(|| format!("reading {}", describe_path.display()))?;
            let describe: DescribeJson = serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing {}", describe_path.display()))?;

            candidates.push((kind, parsed_ver, describe));
        }
    }

    if candidates.is_empty() {
        return Ok(None);
    }

    // Pick the highest semver among all candidates.
    candidates.sort_by(|a, b| a.1.cmp(&b.1));
    let (kind, _ver, describe) = candidates.into_iter().next_back().unwrap();
    Ok(Some((kind, describe)))
}

fn format_kind_display(kind: ExtensionKind) -> &'static str {
    match kind {
        ExtensionKind::Design => "DesignExtension",
        ExtensionKind::Bundle => "BundleExtension",
        ExtensionKind::Deploy => "DeployExtension",
        ExtensionKind::Provider => "ProviderExtension",
    }
}

fn render_info(kind: ExtensionKind, d: &DescribeJson) {
    println!("Kind: {}", format_kind_display(kind));
    println!("Name: {}", d.metadata.id);
    println!("Version: {}", d.metadata.version);
    println!("License: {}", d.metadata.license);
    println!("Summary: {}", d.metadata.summary);

    if kind == ExtensionKind::Provider
        && let Some(gtpack) = &d.runtime.gtpack
    {
        println!("Runtime pack: {}", gtpack.pack_id);
        println!("Component version: {}", gtpack.component_version);
    }

    if !d.capabilities.offered.is_empty() {
        let caps: Vec<String> = d
            .capabilities
            .offered
            .iter()
            .map(|c| c.id.as_str().to_owned())
            .collect();
        println!("Capabilities: {}", caps.join(", "));
    }
}
