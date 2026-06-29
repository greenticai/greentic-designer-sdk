use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::config::resolve_registry_url;
use greentic_extension_sdk_registry::credentials::Credentials;

/// Env var checked for a token in non-interactive/CI logins.
const TOKEN_ENV: &str = "GTDX_TOKEN";

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Registry to log in to (defaults to the configured default registry).
    #[arg(long)]
    pub registry: Option<String>,

    /// Access token. When omitted, falls back to the `GTDX_TOKEN` env var, then
    /// to an interactive prompt. Supplying it makes login non-interactive (CI).
    #[arg(long)]
    pub token: Option<String>,

    /// Do not open the browser to the token-creation page.
    #[arg(long)]
    pub no_browser: bool,
}

#[derive(ClapArgs, Debug)]
pub struct LogoutArgs {
    #[arg(long)]
    pub registry: Option<String>,
}

pub fn run_login(args: &Args, home: &Path) -> anyhow::Result<()> {
    let cfg = super::load_config(home)?;
    let registry_name = args
        .registry
        .clone()
        .unwrap_or_else(|| cfg.default.registry.clone());

    let registry_url = resolve_registry_url(&cfg, &registry_name).ok_or_else(|| {
        anyhow::anyhow!(
            "registry '{registry_name}' has no URL configured. Add it first: gtdx registries add {registry_name} <url>"
        )
    })?;

    let token = obtain_token(args, &registry_name, &registry_url)?;
    if token.trim().is_empty() {
        anyhow::bail!("no token provided; nothing was saved");
    }

    let creds_path = home.join("credentials.toml");
    let mut creds = Credentials::load(&creds_path).unwrap_or_default();
    creds.set(&registry_name, token.trim());
    creds
        .save(&creds_path)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("✓ logged in to {registry_name} ({registry_url})");
    Ok(())
}

/// Resolve the token from (in priority order) `--token`, `$GTDX_TOKEN`, or an
/// interactive prompt. The interactive path opens the registry in the browser
/// first (unless `--no-browser`) so the user can create an access token, then
/// pastes it back — a device-login-style flow against the current token API.
fn obtain_token(args: &Args, registry_name: &str, registry_url: &str) -> anyhow::Result<String> {
    if let Some(token) = &args.token {
        return Ok(token.clone());
    }
    if let Ok(token) = std::env::var(TOKEN_ENV)
        && !token.is_empty()
    {
        println!("Using token from ${TOKEN_ENV}");
        return Ok(token);
    }

    println!("Logging in to {registry_name} ({registry_url})");
    if args.no_browser {
        println!("Create an access token, then paste it below.");
    } else {
        match open_in_browser(registry_url) {
            Ok(()) => println!(
                "Opened {registry_url} in your browser. Create an access token there, then paste it below."
            ),
            Err(_) => println!(
                "Could not open a browser. Visit {registry_url} to create an access token, then paste it below."
            ),
        }
    }

    let token = dialoguer::Password::new()
        .with_prompt(format!("Access token for {registry_name}"))
        .interact()?;
    Ok(token)
}

/// Best-effort cross-platform browser open. Returns an error if the platform
/// opener could not be launched; callers treat that as non-fatal.
fn open_in_browser(url: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let (program, args): (&str, &[&str]) = ("open", &[]);
    #[cfg(target_os = "windows")]
    let (program, args): (&str, &[&str]) = ("cmd", &["/C", "start", ""]);
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let (program, args): (&str, &[&str]) = ("xdg-open", &[]);

    let status = std::process::Command::new(program)
        .args(args)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("browser opener exited with {status}")
    }
}

pub fn run_logout(args: &LogoutArgs, home: &Path) -> anyhow::Result<()> {
    let cfg = super::load_config(home)?;
    let reg_name = args.registry.as_deref().unwrap_or(&cfg.default.registry);
    let creds_path = home.join("credentials.toml");
    let mut creds = Credentials::load(&creds_path).unwrap_or_default();
    if creds.remove(reg_name).is_some() {
        creds
            .save(&creds_path)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        println!("✓ logged out of {reg_name}");
    } else {
        println!("no credentials for {reg_name}");
    }
    Ok(())
}
