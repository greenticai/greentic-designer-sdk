mod commands;
mod dev;
mod publish;
mod scaffold;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gtdx", version, about = "Greentic Designer Extensions CLI")]
struct Cli {
    /// Override greentic home directory (default: ~/.greentic)
    #[arg(long, env = "GREENTIC_HOME", global = true)]
    home: Option<std::path::PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate an extension directory against the describe.json schema
    Validate(commands::validate::Args),
    /// List installed extensions
    List(commands::list::Args),
    /// Install an extension from a registry or local .gtxpack
    Install(commands::install::Args),
    /// Generate an ed25519 keypair for signing extension artifacts
    Keygen(commands::keygen::Args),
    /// Remove an installed extension
    Uninstall(commands::uninstall::Args),
    /// Search a registry
    Search(commands::search::Args),
    /// Show metadata for an extension
    Info(commands::info::Args),
    /// Scaffold a new extension project
    New(commands::new::Args),
    /// Run the developer inner-loop: rebuild, pack, and install on source change
    Dev(commands::dev::Args),
    /// Publish an extension to a registry
    Publish(commands::publish::Args),
    /// Log in to a registry (stores token at ~/.greentic/credentials.toml)
    Login(commands::login::Args),
    /// Log out of a registry
    Logout(commands::login::LogoutArgs),
    /// Show/modify configured registries
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
    /// Print version
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let home = resolve_home(cli.home)?;

    match cli.command {
        Command::Validate(args) => commands::validate::run(&args, &home),
        Command::List(args) => commands::list::run(args, &home),
        Command::Install(args) => commands::install::run(args, &home).await,
        Command::Keygen(args) => commands::keygen::run(&args, &home),
        Command::Uninstall(args) => commands::uninstall::run(&args, &home),
        Command::Search(args) => commands::search::run(args, &home).await,
        Command::Info(args) => commands::info::run(&args, &home),
        Command::New(args) => commands::new::run(&args, &home),
        Command::Dev(args) => commands::dev::run(args, &home).await,
        Command::Publish(args) => commands::publish::run(args, &home).await,
        Command::Login(args) => commands::login::run_login(&args, &home),
        Command::Logout(args) => commands::login::run_logout(&args, &home),
        Command::Registries(args) => commands::registries::run(args, &home),
        Command::Doctor(args) => commands::doctor::run(args, &home).await,
        Command::Enable(args) => commands::enable::run(&args, &home),
        Command::Disable(args) => commands::disable::run(&args, &home),
        Command::Sign(args) => commands::sign::run(&args, &home),
        Command::Verify(args) => commands::verify::run(&args, &home),
        Command::Version => {
            println!("gtdx {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
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
