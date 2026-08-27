use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::{ExtensionRegistry, SearchQuery};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Search term (partial-match on extension name). If omitted, lists everything the registry exposes.
    pub query: Option<String>,
    #[arg(long)]
    pub registry: Option<String>,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
}

/// Resolve the `--kind` argument to a filter.
///
/// Derives from `ExtensionKind::ALL` rather than matching literals: the
/// hand-written match omitted `provider`, so `--kind provider` answered
/// "unknown kind: provider" for a kind that has existed since 1.2.0.
fn parse_kind_arg(
    kind: Option<&str>,
) -> anyhow::Result<Option<greentic_extension_sdk_contract::ExtensionKind>> {
    use greentic_extension_sdk_contract::ExtensionKind;

    match kind {
        None => Ok(None),
        Some(s) => ExtensionKind::from_dir_name(s).map(Some).ok_or_else(|| {
            let known: Vec<&str> = ExtensionKind::ALL.iter().map(|k| k.dir_name()).collect();
            anyhow::anyhow!("unknown kind: {s} (known kinds: {})", known.join(", "))
        }),
    }
}

pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    // Shared with publish/yank: reads the token_env variable AND
    // ~/.greentic/credentials.toml, and falls back to the built-in
    // greentic-store URL. Search previously did neither, so `gtdx login`
    // did not authenticate a search and the canonical store 404'd as
    // "no such registry" until someone ran `gtdx registries add`.
    let reg = super::resolve_store_registry(args.registry.as_deref(), home)?;

    let kind = parse_kind_arg(args.kind.as_deref())?;

    let results = reg
        .search(SearchQuery {
            kind,
            query: args.query,
            limit: args.limit,
            ..Default::default()
        })
        .await?;
    if results.is_empty() {
        println!("(no extensions match)");
        return Ok(());
    }
    for r in results {
        println!(
            "{:<40}  {:>10}  {:?}  {}",
            r.name, r.latest_version, r.kind, r.summary
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_kind_arg;
    use greentic_extension_sdk_contract::ExtensionKind;

    /// `--kind provider` answered "unknown kind: provider" because the match
    /// hand-listed four dir names. Every kind must parse.
    #[test]
    fn every_kind_dir_name_parses() {
        for kind in ExtensionKind::ALL.iter().copied() {
            let parsed = parse_kind_arg(Some(kind.dir_name()))
                .unwrap_or_else(|e| panic!("{} should parse: {e}", kind.dir_name()));
            assert_eq!(parsed, Some(kind));
        }
    }

    #[test]
    fn no_kind_means_no_filter() {
        assert_eq!(parse_kind_arg(None).expect("None is valid"), None);
    }

    #[test]
    fn an_unknown_kind_is_an_error() {
        let err = parse_kind_arg(Some("nonsense")).expect_err("unknown kind must error");
        assert!(
            err.to_string().contains("nonsense"),
            "the error should name the offending input, got: {err}"
        );
    }
}
