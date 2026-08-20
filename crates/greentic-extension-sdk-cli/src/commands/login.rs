use std::path::Path;
use std::time::{Duration, Instant};

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::config::resolve_registry_url;
use greentic_extension_sdk_registry::credentials::Credentials;
use serde::{Deserialize, Serialize};

/// Env var checked for a token in non-interactive/CI logins.
const TOKEN_ENV: &str = "GTDX_TOKEN";

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Registry to log in to (defaults to the configured default registry).
    #[arg(long)]
    pub registry: Option<String>,

    /// Access token. When omitted, falls back to the `GTDX_TOKEN` env var, then
    /// to browser login. Supplying it makes login non-interactive (CI).
    #[arg(long)]
    pub token: Option<String>,

    /// Do not open the browser automatically (the URL is printed instead).
    #[arg(long)]
    pub no_browser: bool,

    /// Skip device login and paste a token manually instead.
    #[arg(long)]
    pub paste: bool,
}

#[derive(ClapArgs, Debug)]
pub struct LogoutArgs {
    #[arg(long)]
    pub registry: Option<String>,
}

pub async fn run_login(args: &Args, home: &Path) -> anyhow::Result<()> {
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

    let token = resolve_login_token(args, &registry_name, &registry_url).await?;
    if token.trim().is_empty() {
        anyhow::bail!("no token provided; nothing was saved");
    }

    let creds_path = home.join("credentials.toml");
    // `Credentials::load` returns Ok(default) when the file is *absent* and
    // Err only when it exists and cannot be read or parsed. `unwrap_or_default`
    // conflated the two: one corrupt byte produced an empty set, `save` then
    // truncated the file, and every *other* registry's token was permanently
    // gone — while printing "✓ logged in". Refuse instead.
    let mut creds = Credentials::load(&creds_path).map_err(|e| {
        anyhow::anyhow!(
            "{} exists but could not be read ({e}); move it aside and re-run to \
             recreate it — refusing to overwrite it and lose other registries' tokens",
            creds_path.display()
        )
    })?;
    creds.set(&registry_name, token.trim());
    creds
        .save(&creds_path)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("✓ logged in to {registry_name} ({registry_url})");
    Ok(())
}

/// Resolve the access token from (in priority order) `--token`, `$GTDX_TOKEN`,
/// browser device login, or a manual token paste. Device login is the default
/// interactive path and transparently falls back to paste when the store does
/// not implement it.
async fn resolve_login_token(
    args: &Args,
    registry_name: &str,
    registry_url: &str,
) -> anyhow::Result<String> {
    if let Some(token) = &args.token {
        return Ok(token.clone());
    }
    if let Ok(token) = std::env::var(TOKEN_ENV)
        && !token.is_empty()
    {
        println!("Using token from ${TOKEN_ENV}");
        return Ok(token);
    }

    if !args.paste {
        match device_login(registry_url, args.no_browser).await {
            Ok(Some(token)) => return Ok(token),
            Ok(None) => {
                eprintln!(
                    "note: {registry_url} does not support device login; falling back to manual token paste"
                );
            }
            Err(err) => return Err(err),
        }
    }

    paste_token(registry_name, registry_url, args.no_browser)
}

// ---- Device Authorization Grant (RFC 8628) client ----

#[derive(Serialize)]
struct DeviceCodeRequest {
    client_name: Option<String>,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: i64,
    interval: i64,
}

#[derive(Serialize)]
struct DeviceTokenRequest<'a> {
    device_code: &'a str,
}

#[derive(Deserialize)]
struct DeviceTokenResponse {
    status: String,
    access_token: Option<String>,
}

/// Run the device-authorization flow against `registry_url`. Returns
/// `Ok(Some(token))` on approval, `Ok(None)` when the store has no device
/// endpoints (caller falls back to paste), and `Err` on denial/expiry/timeout.
async fn device_login(registry_url: &str, no_browser: bool) -> anyhow::Result<Option<String>> {
    // The registry crate gates every store call on `validate_registry_url` and
    // builds its client with redirects disabled *specifically* so an HTTPS→HTTP
    // 3xx cannot downgrade. This flow bypassed all of it with a bare
    // `Client::new()`, so an `http://` registry entry — which `registries add`
    // accepts without complaint — sent the device code out and the bearer token
    // back in cleartext.
    greentic_extension_sdk_registry::store::validate_registry_url(
        registry_url,
        crate::registry_security::insecure_registry_opt_in(),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow::anyhow!("build http client: {e}"))?;
    let code: DeviceCodeResponse = {
        let response = client
            .post(format!("{registry_url}/api/v1/auth/device/code"))
            .json(&DeviceCodeRequest {
                client_name: Some(client_name()),
            })
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response.error_for_status()?.json().await?
    };

    println!();
    println!("To sign in, visit:");
    println!("    {}", code.verification_uri);
    println!("and enter the code:  {}", code.user_code);
    println!();
    if !no_browser && open_in_browser(&code.verification_uri_complete).is_ok() {
        println!(
            "(opened your browser to {})",
            code.verification_uri_complete
        );
    }
    println!("Waiting for approval…");

    let token_url = format!("{registry_url}/api/v1/auth/device/token");
    let interval = Duration::from_secs(u64::try_from(code.interval).unwrap_or(5).max(1));
    let ttl = Duration::from_secs(u64::try_from(code.expires_in).unwrap_or(900).max(1));
    let deadline = Instant::now() + ttl;

    loop {
        tokio::time::sleep(interval).await;
        if Instant::now() > deadline {
            anyhow::bail!("device login timed out; run `gtdx login` again");
        }
        let poll: DeviceTokenResponse = client
            .post(&token_url)
            .json(&DeviceTokenRequest {
                device_code: &code.device_code,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        match poll.status.as_str() {
            "approved" => {
                let token = poll
                    .access_token
                    .ok_or_else(|| anyhow::anyhow!("store approved login but returned no token"))?;
                return Ok(Some(token));
            }
            "pending" => {}
            "denied" => anyhow::bail!("login was denied in the browser"),
            "expired" => anyhow::bail!("device code expired; run `gtdx login` again"),
            other => anyhow::bail!("unexpected device login status: {other}"),
        }
    }
}

/// A human label for this client, shown on the browser approval page.
fn client_name() -> String {
    let host = std::env::var("HOSTNAME")
        .ok()
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var("HOST").ok().filter(|h| !h.is_empty()));
    match host {
        Some(host) => format!("gtdx on {host}"),
        None => "gtdx CLI".to_string(),
    }
}

/// Manual token paste: open the store so the user can mint a token, then read
/// it from a hidden prompt.
fn paste_token(
    registry_name: &str,
    registry_url: &str,
    no_browser: bool,
) -> anyhow::Result<String> {
    println!("Logging in to {registry_name} ({registry_url})");
    if no_browser {
        println!("Create an access token, then paste it below.");
    } else {
        match open_in_browser(registry_url) {
            Ok(()) => println!(
                "Opened {registry_url} in your browser. Create an access token, then paste it below."
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
    // Belt and braces on every platform: the URL is registry-controlled, so
    // refuse anything that is not an ordinary http(s) address before handing it
    // to a platform opener.
    if !(url.starts_with("https://") || url.starts_with("http://"))
        || url.chars().any(|c| {
            c.is_control() || matches!(c, '"' | '\'' | '&' | '|' | '^' | '<' | '>' | '`' | '$')
        })
    {
        anyhow::bail!("refusing to open a non-http(s) or unsafe verification URL: {url:?}");
    }
    #[cfg(target_os = "macos")]
    let (program, args): (&str, &[&str]) = ("open", &[]);
    // Not `cmd /C start`: Rust's Windows argument quoting only quotes on
    // whitespace, and `cmd.exe` splits on metacharacters — so a
    // registry-supplied `https://x/a&calc.exe` executed the payload. This flow
    // takes its URL straight from the registry's JSON response.
    // `rundll32 url.dll,FileProtocolHandler` takes the URL as a plain argument
    // with no shell in between.
    #[cfg(target_os = "windows")]
    let (program, args): (&str, &[&str]) = ("rundll32", &["url.dll,FileProtocolHandler"]);
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
    // `Credentials::load` returns Ok(default) when the file is *absent* and
    // Err only when it exists and cannot be read or parsed. `unwrap_or_default`
    // conflated the two: one corrupt byte produced an empty set, `save` then
    // truncated the file, and every *other* registry's token was permanently
    // gone — while printing "✓ logged in". Refuse instead.
    let mut creds = Credentials::load(&creds_path).map_err(|e| {
        anyhow::anyhow!(
            "{} exists but could not be read ({e}); move it aside and re-run to \
             recreate it — refusing to overwrite it and lose other registries' tokens",
            creds_path.display()
        )
    })?;
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
