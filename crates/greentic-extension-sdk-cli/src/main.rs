#![forbid(unsafe_code)]

mod commands;
mod dev;
mod icon;
mod publish;
mod registry_security;
mod scaffold;
mod signing;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gtdx", version, about = "Greentic Designer Extensions CLI")]
struct Cli {
    /// Override greentic home directory (default: ~/.greentic)
    #[arg(long, env = "GREENTIC_HOME", global = true)]
    home: Option<std::path::PathBuf>,

    /// Increase log verbosity: --verbose for info, repeat for debug.
    /// Warnings are shown by default; `RUST_LOG` overrides both.
    ///
    /// Long-only deliberately: `gtdx new -v <VERSION>` already claims `-v`, so a
    /// global short form fails clap's uniqueness assert at startup. Reclaiming
    /// `-v` for verbosity means changing `new -v`, which is a user-facing break
    /// and belongs with the wider flag-consistency cleanup, not here.
    #[arg(long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate an extension directory against the describe.json schema
    Validate(commands::validate::Args),
    /// Register a component-tool by URL against greentic-designer-admin
    Component(commands::component::Args),
    /// List installed extensions
    #[command(alias = "ls")]
    List(commands::list::Args),
    /// Install an extension from a registry or local .gtxpack
    #[command(alias = "i")]
    Install(commands::install::Args),
    /// Generate an ed25519 keypair for signing extension artifacts
    Keygen(commands::keygen::Args),
    /// Remove an installed extension
    #[command(aliases = ["rm", "remove"])]
    Uninstall(commands::uninstall::Args),
    /// Search a registry
    Search(commands::search::Args),
    /// Show metadata for an extension
    Info(commands::info::Args),
    /// Scaffold a new extension project
    New(commands::new::Args),
    /// Generate a `DesignExtension` connector from an `OpenAPI` 3.0 spec
    Openapi(commands::openapi::Args),
    /// Run the developer inner-loop: rebuild, pack, and install on source change
    Dev(commands::dev::Args),
    /// Publish an extension to a registry
    Publish(commands::publish::Args),
    /// Log in to a registry (stores token at ~/.greentic/credentials.toml)
    Login(commands::login::Args),
    /// Log out of a registry
    Logout(commands::login::LogoutArgs),
    /// Show/modify configured registries
    #[command(alias = "reg")]
    Registries(commands::registries::Args),
    /// Diagnose installed extensions
    Doctor(commands::doctor::Args),
    /// Enable an installed extension
    Enable(commands::enable::EnableArgs),
    /// Disable an installed extension
    Disable(commands::disable::DisableArgs),
    /// Sign a describe.json in-place with ed25519
    Sign(commands::sign::Args),
    /// Verify an extension's signature (file, directory, or .gtxpack)
    Verify(commands::verify::Args),
    /// Lint a describe.json for cross-field invariants beyond JSON Schema
    Lint(commands::lint::Args),
    /// Check installed extensions for available updates
    Outdated(commands::outdated::Args),
    /// Update installed extensions to the latest permitted version
    Update(commands::update::Args),
    /// Withdraw a published version: hidden from the version list and never
    /// selected as latest, but still downloadable for existing pins
    Yank(commands::yank::Args),
    /// Reverse a yank, putting a version back in circulation
    Unyank(commands::yank::UnyankArgs),
    /// Print version
    ///
    /// Hidden: `--version` already does this. Kept so existing scripts and
    /// muscle memory keep working, but it should not clutter the command list.
    #[command(hide = true)]
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse before initialising logging so `-v` can set the level. `parse`
    // itself never logs, and exits directly on a usage error.
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    let home = resolve_home(cli.home)?;

    match cli.command {
        Command::Validate(args) => commands::validate::run(&args, &home),
        Command::Component(args) => commands::component::run(args, &home).await,
        Command::List(args) => commands::list::run(args, &home),
        Command::Install(args) => commands::install::run(args, &home).await,
        Command::Keygen(args) => commands::keygen::run(&args, &home),
        Command::Uninstall(args) => commands::uninstall::run(&args, &home),
        Command::Search(args) => commands::search::run(args, &home).await,
        Command::Info(args) => commands::info::run(&args, &home),
        Command::New(args) => commands::new::run(&args, &home),
        Command::Openapi(args) => commands::openapi::run(&args),
        Command::Dev(args) => commands::dev::run(args, &home).await,
        Command::Publish(args) => commands::publish::run(args, &home).await,
        Command::Login(args) => commands::login::run_login(&args, &home).await,
        Command::Logout(args) => commands::login::run_logout(&args, &home),
        Command::Registries(args) => commands::registries::run(args, &home),
        Command::Doctor(args) => commands::doctor::run(args, &home).await,
        Command::Enable(args) => commands::enable::run(&args, &home),
        Command::Disable(args) => commands::disable::run(&args, &home),
        Command::Sign(args) => commands::sign::run(&args, &home),
        Command::Verify(args) => commands::verify::run(&args, &home),
        Command::Lint(args) => commands::lint::run(&args, &home),
        Command::Outdated(args) => commands::outdated::run(args, &home).await,
        Command::Update(args) => commands::update::run(args, &home).await,
        Command::Yank(args) => commands::yank::run_yank(args, &home).await,
        Command::Unyank(args) => commands::yank::run_unyank(args, &home).await,
        Command::Version => {
            println!("gtdx {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

/// Install the tracing subscriber with a WARN floor.
///
/// The previous form used a bare `EnvFilter::from_default_env()`, which with
/// `RUST_LOG` unset — the normal case — resolves to ERROR only. That silenced
/// every `tracing::warn!` in the workspace, including the two notices in
/// `registry::verify` that exist specifically to tell the user a signature
/// check was bypassed or that `--trust strict` is anchored only by a
/// trust-on-first-use pin. Security notices a user never sees are not notices.
///
/// `RUST_LOG` still wins when set, so existing debugging workflows are intact.
fn init_tracing(verbose: u8) {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(default_directive(verbose).into())
                .from_env_lossy(),
        )
        .init();
}

/// Map `-v` occurrences to the log level used when `RUST_LOG` is unset.
fn default_directive(verbose: u8) -> tracing_subscriber::filter::LevelFilter {
    use tracing_subscriber::filter::LevelFilter;
    match verbose {
        0 => LevelFilter::WARN,
        1 => LevelFilter::INFO,
        _ => LevelFilter::DEBUG,
    }
}

fn resolve_home(override_path: Option<std::path::PathBuf>) -> anyhow::Result<std::path::PathBuf> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    directories::BaseDirs::new()
        .map(|d| d.home_dir().join(".greentic"))
        .ok_or_else(|| anyhow::anyhow!("no home directory"))
}

#[cfg(test)]
mod tracing_tests {
    use super::default_directive;
    use tracing_subscriber::filter::LevelFilter;

    /// The regression guard. A default of ERROR (what `from_default_env()`
    /// resolves to on its own) silently drops every `tracing::warn!` in the
    /// workspace — including `registry::verify`'s notices that a signature
    /// check was bypassed, or that `--trust strict` is only TOFU-anchored.
    #[test]
    fn warnings_are_visible_without_rust_log() {
        assert_eq!(default_directive(0), LevelFilter::WARN);
        assert!(
            default_directive(0) >= LevelFilter::WARN,
            "default level must not sit below WARN"
        );
    }

    #[test]
    fn verbosity_flags_step_up_the_level() {
        assert_eq!(default_directive(1), LevelFilter::INFO);
        assert_eq!(default_directive(2), LevelFilter::DEBUG);
        assert_eq!(default_directive(9), LevelFilter::DEBUG);
    }
}
