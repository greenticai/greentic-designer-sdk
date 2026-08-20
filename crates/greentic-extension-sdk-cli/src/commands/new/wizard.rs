//! Interactive `gtdx new` wizard.
//!
//! Guides the user through the same inputs the flag-driven path accepts, so a
//! new extension can be scaffolded without memorising the full command. Any
//! flags already supplied on the command line are used as prompt defaults.

use std::path::PathBuf;

use dialoguer::{Confirm, Input, Select};

use super::{Args, Resolved, detect_git_author, is_reverse_dns};
use crate::scaffold::Kind;

/// Extension kinds offered in the picker, each with a one-line description.
/// Ordered most-common-first; `mcp` is surfaced near the top because it is the
/// flow-capable `wasix:mcp/router` style most new integrations want.
const KIND_CHOICES: &[(Kind, &str, &str)] = &[
    (
        Kind::Mcp,
        "mcp",
        "MCP tools an agent can call (wasix:mcp/router)",
    ),
    (
        Kind::Design,
        "design",
        "Designer-time tools, validation, prompting, knowledge",
    ),
    (
        Kind::WasmComponent,
        "wasm-component",
        "Register a node type in the flow canvas",
    ),
    (
        Kind::Provider,
        "provider",
        "Messaging channels (events need a manual world swap)",
    ),
    (
        Kind::Llm,
        "llm",
        "Design extension that calls an LLM over HTTP",
    ),
    (
        Kind::Bundle,
        "bundle",
        "Recipes that render a session bundle",
    ),
    (
        Kind::Deploy,
        "deploy",
        "Deploy targets: deploy / poll / rollback",
    ),
];

/// Common SPDX ids offered by the license picker, plus a free-text escape.
const LICENSE_CHOICES: &[&str] = &[
    "Apache-2.0",
    "MIT",
    "MPL-2.0",
    "AGPL-3.0-or-later",
    "LicenseRef-Proprietary",
];

/// A prompt the user escaped out of (Esc / q / Ctrl-D).
///
/// Cancelling is a decision, not a failure: `run` turns this into a clean exit
/// rather than an `Error:` line and a non-zero status.
struct Cancelled;

type Step<T> = Result<T, Cancelled>;

/// Run the interactive wizard, falling back to `args` values as defaults.
#[allow(clippy::unnecessary_wraps)] // signature mirrors `resolve_from_flags`
pub(super) fn run(args: &Args) -> anyhow::Result<Resolved> {
    // Toolchain checks first. They used to run after every prompt, so a user
    // without cargo-component answered six questions before being told.
    super::print_toolchain_preflight();

    println!("\ngtdx new — interactive wizard (press Enter to accept defaults, Esc to cancel)\n");

    if let Ok(resolved) = collect(args) {
        return Ok(resolved);
    }
    println!("\nCancelled. Nothing was written.");
    std::process::exit(0);
}

fn collect(args: &Args) -> Step<Resolved> {
    // Kind first: it decides what every later answer means.
    let kind = prompt_kind(args)?;
    let dir = args.dir.clone();
    // Ask for the name and settle the destination together: a conflict is
    // raised while the user can still pick another name, rather than as a hard
    // preflight failure after the whole questionnaire.
    let (name, target, force) = loop {
        let name = prompt_name(args)?;
        match resolve_target(&name, dir.as_deref(), args.force)? {
            TargetChoice::Use { target, force } => break (name, target, force),
            TargetChoice::Rename => {}
        }
    };

    let from_openapi = prompt_openapi_seed(args, kind)?;
    let id = prompt_id(args, &name)?;
    let version = prompt_version(args)?;
    let author = prompt_author(args)?;
    let license = prompt_license(args)?;

    print_summary(
        &name, kind, &id, &version, &author, &license, &target, force,
    );
    if !confirm("Create this extension?", true)? {
        return Err(Cancelled);
    }
    print_equivalent_command(&name, kind, &id, &version, &license);

    Ok(Resolved {
        name,
        kind,
        id,
        version,
        author,
        license,
        no_git: args.no_git,
        dir,
        force,
        node_type_id: args.node_type_id.clone(),
        label: args.label.clone(),
        from_openapi,
        // The wizard just confirmed the overwrite interactively; don't ask twice.
        assume_yes: true,
        came_from_wizard: true,
    })
}

/// Decide where the project goes, resolving an existing-directory conflict
/// inline instead of failing preflight after all the questions.
enum TargetChoice {
    Use { target: PathBuf, force: bool },
    Rename,
}

fn resolve_target(name: &str, dir: Option<&std::path::Path>, force: bool) -> Step<TargetChoice> {
    let target = dir.map_or_else(|| PathBuf::from(name), std::path::Path::to_path_buf);
    let occupied = target.read_dir().is_ok_and(|mut d| d.next().is_some());
    if !occupied || force {
        return Ok(TargetChoice::Use { target, force });
    }

    println!();
    println!("  {} already exists and is not empty.", target.display());
    let choice = select(
        "What would you like to do?",
        &[
            "Pick a different name".to_string(),
            format!("Overwrite it (deletes {})", target.display()),
            "Cancel".to_string(),
        ],
        0,
    )?;
    match choice {
        0 => Ok(TargetChoice::Rename),
        1 => Ok(TargetChoice::Use {
            target,
            force: true,
        }),
        _ => Err(Cancelled),
    }
}

fn prompt_kind(args: &Args) -> Step<Kind> {
    let labels: Vec<String> = KIND_CHOICES
        .iter()
        .map(|(_, name, desc)| format!("{name:<15} {desc}"))
        .collect();
    let default_index = KIND_CHOICES
        .iter()
        .position(|(kind, _, _)| *kind == args.kind)
        .unwrap_or(0);
    let selected = select("What are you building?", &labels, default_index)?;
    Ok(KIND_CHOICES[selected].0)
}

fn prompt_name(args: &Args) -> Step<String> {
    let mut input = Input::<String>::new().with_prompt("Project name (kebab-case)");
    if let Some(existing) = &args.name {
        input = input.default(existing.clone());
    }
    let name = input
        .validate_with(validate_name_input)
        .interact_text()
        .map_err(|_| Cancelled)?;
    Ok(name.trim().to_string())
}

/// If kind is MCP and `--from-openapi` was not already supplied, ask whether
/// the user wants to seed from an `OpenAPI` spec. Returns the spec path if yes.
fn prompt_openapi_seed(args: &Args, kind: Kind) -> Step<Option<PathBuf>> {
    if args.from_openapi.is_some() || kind != Kind::Mcp {
        return Ok(args.from_openapi.clone());
    }
    if !confirm("Seed this MCP extension from an OpenAPI spec?", false)? {
        return Ok(None);
    }
    let path: String = Input::new()
        .with_prompt("OpenAPI spec path")
        .interact_text()
        .map_err(|_| Cancelled)?;
    Ok(Some(PathBuf::from(path)))
}

fn prompt_id(args: &Args, name: &str) -> Step<String> {
    // `name` is now guaranteed kebab-case, so this default is always valid —
    // it used to be offered even when it could never pass validation, which
    // trapped the user in an unescapable re-prompt loop.
    let default_id = args
        .id
        .clone()
        .unwrap_or_else(|| format!("com.example.{name}"));
    let id = Input::<String>::new()
        .with_prompt("Extension id (reverse-DNS)")
        .default(default_id)
        .validate_with(validate_id_input)
        .interact_text()
        .map_err(|_| Cancelled)?;
    Ok(id.trim().to_string())
}

fn prompt_version(args: &Args) -> Step<String> {
    let version = Input::<String>::new()
        .with_prompt("Version")
        .default(args.version.clone())
        .validate_with(validate_version_input)
        .interact_text()
        .map_err(|_| Cancelled)?;
    Ok(version.trim().to_string())
}

fn prompt_author(args: &Args) -> Step<String> {
    let default_author = args.author.clone().unwrap_or_else(detect_git_author);
    let author = Input::<String>::new()
        .with_prompt("Author")
        .default(default_author)
        .validate_with(validate_author_input)
        .interact_text()
        .map_err(|_| Cancelled)?;
    Ok(author.trim().to_string())
}

fn prompt_license(args: &Args) -> Step<String> {
    let mut items: Vec<String> = LICENSE_CHOICES.iter().map(|s| (*s).to_string()).collect();
    items.push("Other…".to_string());
    let default_index = LICENSE_CHOICES
        .iter()
        .position(|l| *l == args.license)
        .unwrap_or(0);
    let selected = select("License", &items, default_index)?;
    if selected < LICENSE_CHOICES.len() {
        return Ok(LICENSE_CHOICES[selected].to_string());
    }
    let license = Input::<String>::new()
        .with_prompt("License (SPDX id)")
        .validate_with(validate_license_input)
        .interact_text()
        .map_err(|_| Cancelled)?;
    Ok(license.trim().to_string())
}

// ---- dialoguer wrappers that treat Esc as "cancel", not "error" ----
//
// `Select::interact` / `Confirm::interact` disable the quit key, so Esc did
// nothing at all and the wizard simply sat there with no way out but Ctrl-C —
// which also left the terminal cursor hidden. `*_opt` enables it and reports
// the escape as `None`.

fn select(prompt: &str, items: &[String], default: usize) -> Step<usize> {
    Select::new()
        .with_prompt(prompt)
        .items(items)
        .default(default)
        .interact_opt()
        .map_err(|_| Cancelled)?
        .ok_or(Cancelled)
}

fn confirm(prompt: &str, default: bool) -> Step<bool> {
    Confirm::new()
        .with_prompt(prompt)
        .default(default)
        .interact_opt()
        .map_err(|_| Cancelled)?
        .ok_or(Cancelled)
}

#[allow(clippy::too_many_arguments)]
fn print_summary(
    name: &str,
    kind: Kind,
    id: &str,
    version: &str,
    author: &str,
    license: &str,
    target: &std::path::Path,
    force: bool,
) {
    println!("\nAbout to scaffold:");
    println!("  name     {name}");
    println!("  kind     {}", kind.as_str());
    println!("  id       {id}");
    println!("  version  {version}");
    println!("  author   {author}");
    println!("  license  {license}");
    // The destination was never shown, so the user confirmed a write — and
    // under --force a recursive delete — without seeing where it landed.
    println!("  target   {}", target.display());
    if force {
        println!("  ⚠ {} will be REMOVED first", target.display());
    }
}

/// Print the flag form of what the wizard just collected.
///
/// Teaches the non-interactive surface and makes the run reproducible in CI —
/// the cheapest affordance the wizard was missing.
fn print_equivalent_command(name: &str, kind: Kind, id: &str, version: &str, license: &str) {
    println!("\nNext time, skip the wizard with:");
    println!(
        "  gtdx new {name} -k {} -i {id} -v {version} --license {license} -y",
        kind.as_str()
    );
}

// dialoguer's `Validate<String>` bound forces a `&String` receiver here.
#[allow(clippy::ptr_arg)]
fn validate_name_input(input: &String) -> Result<(), String> {
    // Same rule the flag-driven path enforces, so the wizard cannot produce a
    // name `gtdx new <name>` would reject — and vice versa.
    // `validate_name` already appends a `try "…"` suggestion where one helps;
    // don't append a second one.
    super::validate_name(input.trim())
}

#[allow(clippy::ptr_arg)]
fn validate_author_input(input: &String) -> Result<(), String> {
    if input.trim().is_empty() {
        // `allow_empty(true)` used to let this through, yielding `authors = [""]`.
        Err("author cannot be empty".to_string())
    } else {
        Ok(())
    }
}

#[allow(clippy::ptr_arg)]
fn validate_license_input(input: &String) -> Result<(), String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("license cannot be empty".to_string());
    }
    // Not a full SPDX registry check, but the prompt says "SPDX id" and this
    // rejects the prose people actually type (e.g. "MIT license").
    if trimmed.chars().any(char::is_whitespace) {
        return Err("an SPDX id has no spaces, e.g. Apache-2.0 or MIT".to_string());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '+'))
    {
        return Err("an SPDX id uses only letters, digits, `-`, `.` and `+`".to_string());
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

    /// The wizard must enforce exactly the rule the flag path enforces —
    /// otherwise one accepts names the other rejects, which is how the
    /// permanently-invalid id default (and the path escape) happened.
    #[test]
    fn name_validation_matches_the_flag_path() {
        assert!(validate_name_input(&"my-ext".to_string()).is_ok());
        for bad in ["   ", "my ext", "../escaped", "MyExt", "1foo", "a/b"] {
            assert!(
                validate_name_input(&bad.to_string()).is_err(),
                "{bad} must be rejected"
            );
        }
    }

    /// Regression: the id default is derived from the entered name, so it can
    /// only be valid if the name rule is at least as strict as the id rule.
    /// When it was not, the offered default could never be accepted and the
    /// wizard re-prompted forever with no way out.
    #[test]
    fn default_id_from_any_accepted_name_is_itself_valid() {
        for name in ["my-ext", "ext", "a1", "my-ext-2", "abc123"] {
            assert!(validate_name_input(&name.to_string()).is_ok());
            let default_id = format!("com.example.{name}");
            assert!(
                validate_id_input(&default_id).is_ok(),
                "default id {default_id} is invalid for an accepted name"
            );
        }
    }

    #[test]
    fn author_and_license_are_validated() {
        assert!(validate_author_input(&"Jane".to_string()).is_ok());
        assert!(validate_author_input(&"  ".to_string()).is_err());
        assert!(validate_license_input(&"Apache-2.0".to_string()).is_ok());
        assert!(validate_license_input(&"MIT license".to_string()).is_err());
        assert!(validate_license_input(&String::new()).is_err());
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
