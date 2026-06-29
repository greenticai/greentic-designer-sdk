//! Interactive `gtdx new` wizard.
//!
//! Guides the user through the same inputs the flag-driven path accepts, so a
//! new extension can be scaffolded without memorising the full command. Any
//! flags already supplied on the command line are used as prompt defaults.

use dialoguer::{Confirm, Input, Select};

use super::{Args, Resolved, detect_git_author, is_reverse_dns};
use crate::scaffold::Kind;

/// Extension kinds offered in the picker, each with a one-line description.
/// Ordered most-common-first; `mcp` is surfaced near the top because it is the
/// flow-capable `wasix:mcp/router` style most new integrations want.
const KIND_CHOICES: &[(Kind, &str, &str)] = &[
    (Kind::Design, "design", "Designer/agentic tools (default)"),
    (
        Kind::Mcp,
        "mcp",
        "MCP router (wasix:mcp/router) — usable as flow node + agentic tool",
    ),
    (Kind::Provider, "provider", "Messaging/event provider"),
    (
        Kind::WasmComponent,
        "wasm-component",
        "Generic WASM flow component",
    ),
    (Kind::Llm, "llm", "LLM provider extension"),
    (Kind::Bundle, "bundle", "Bundle extension"),
    (Kind::Deploy, "deploy", "Deploy target extension"),
];

/// Run the interactive wizard, falling back to `args` values as defaults.
pub(super) fn run(args: &Args) -> anyhow::Result<Resolved> {
    println!("gtdx new — interactive wizard (press Enter to accept defaults)\n");

    let name = prompt_name(args)?;
    let kind = prompt_kind(args)?;
    let id = prompt_id(args, &name)?;
    let version = prompt_version(args)?;
    let author = prompt_author(args)?;
    let license = prompt_license(args)?;

    print_summary(&name, kind, &id, &version, &author, &license);
    let confirmed = Confirm::new()
        .with_prompt("Create this extension?")
        .default(true)
        .interact()?;
    if !confirmed {
        anyhow::bail!("cancelled by user");
    }

    Ok(Resolved {
        name,
        kind,
        id,
        version,
        author,
        license,
        no_git: args.no_git,
        dir: args.dir.clone(),
        force: args.force,
        node_type_id: args.node_type_id.clone(),
        label: args.label.clone(),
    })
}

fn prompt_name(args: &Args) -> anyhow::Result<String> {
    let mut input = Input::<String>::new().with_prompt("Project name (kebab-case)");
    if let Some(existing) = &args.name {
        input = input.default(existing.clone());
    }
    let name = input.validate_with(validate_name).interact_text()?;
    Ok(name.trim().to_string())
}

fn prompt_kind(args: &Args) -> anyhow::Result<Kind> {
    let labels: Vec<String> = KIND_CHOICES
        .iter()
        .map(|(_, name, desc)| format!("{name:<15} {desc}"))
        .collect();
    let default_index = KIND_CHOICES
        .iter()
        .position(|(kind, _, _)| *kind == args.kind)
        .unwrap_or(0);
    let selected = Select::new()
        .with_prompt("Extension kind")
        .items(&labels)
        .default(default_index)
        .interact()?;
    Ok(KIND_CHOICES[selected].0)
}

fn prompt_id(args: &Args, name: &str) -> anyhow::Result<String> {
    let default_id = args
        .id
        .clone()
        .unwrap_or_else(|| format!("com.example.{name}"));
    let id = Input::<String>::new()
        .with_prompt("Extension id (reverse-DNS)")
        .default(default_id)
        .validate_with(validate_id_input)
        .interact_text()?;
    Ok(id.trim().to_string())
}

fn prompt_version(args: &Args) -> anyhow::Result<String> {
    let version = Input::<String>::new()
        .with_prompt("Version")
        .default(args.version.clone())
        .validate_with(validate_version_input)
        .interact_text()?;
    Ok(version.trim().to_string())
}

fn prompt_author(args: &Args) -> anyhow::Result<String> {
    let default_author = args.author.clone().unwrap_or_else(detect_git_author);
    let author = Input::<String>::new()
        .with_prompt("Author")
        .default(default_author)
        .allow_empty(true)
        .interact_text()?;
    Ok(author.trim().to_string())
}

fn prompt_license(args: &Args) -> anyhow::Result<String> {
    let license = Input::<String>::new()
        .with_prompt("License (SPDX id)")
        .default(args.license.clone())
        .interact_text()?;
    Ok(license.trim().to_string())
}

fn print_summary(name: &str, kind: Kind, id: &str, version: &str, author: &str, license: &str) {
    println!("\nAbout to scaffold:");
    println!("  name     {name}");
    println!("  kind     {}", kind.as_str());
    println!("  id       {id}");
    println!("  version  {version}");
    println!("  author   {author}");
    println!("  license  {license}");
}

// dialoguer's `Validate<String>` bound forces a `&String` receiver here.
#[allow(clippy::ptr_arg)]
fn validate_name(input: &String) -> Result<(), String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("name cannot be empty".to_string());
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err("name cannot contain whitespace (use kebab-case)".to_string());
    }
    Ok(())
}

#[allow(clippy::ptr_arg)]
fn validate_id_input(input: &String) -> Result<(), String> {
    if is_reverse_dns(input.trim()) {
        Ok(())
    } else {
        Err("must be reverse-DNS, e.g. com.acme.my-ext".to_string())
    }
}

#[allow(clippy::ptr_arg)]
fn validate_version_input(input: &String) -> Result<(), String> {
    semver::Version::parse(input.trim())
        .map(|_| ())
        .map_err(|_| "must be valid semver, e.g. 0.1.0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every scaffoldable `Kind` must be offered by the wizard picker;
    /// otherwise a newly added kind would be silently unreachable interactively.
    #[test]
    fn every_kind_is_offered_in_the_picker() {
        for kind in [
            Kind::Design,
            Kind::Bundle,
            Kind::Deploy,
            Kind::Provider,
            Kind::WasmComponent,
            Kind::Mcp,
            Kind::Llm,
        ] {
            assert!(
                KIND_CHOICES.iter().any(|(choice, _, _)| *choice == kind),
                "Kind {kind:?} is missing from the wizard picker"
            );
        }
    }

    #[test]
    fn name_validation_rejects_empty_and_whitespace() {
        assert!(validate_name(&"my-ext".to_string()).is_ok());
        assert!(validate_name(&"   ".to_string()).is_err());
        assert!(validate_name(&"my ext".to_string()).is_err());
    }

    #[test]
    fn id_validation_enforces_reverse_dns() {
        assert!(validate_id_input(&"com.acme.my-ext".to_string()).is_ok());
        assert!(validate_id_input(&"not-reverse-dns".to_string()).is_err());
    }

    #[test]
    fn version_validation_enforces_semver() {
        assert!(validate_version_input(&"0.1.0".to_string()).is_ok());
        assert!(validate_version_input(&"not-a-version".to_string()).is_err());
    }
}
