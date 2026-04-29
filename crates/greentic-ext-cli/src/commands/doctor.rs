use std::path::Path;

use chrono::{DateTime, Utc};
use clap::Args as ClapArgs;
use greentic_ext_contract::ExtensionKind;
use greentic_ext_registry::credentials::Credentials;
use greentic_ext_registry::storage::Storage;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Skip network probes (offline mode).
    #[arg(long)]
    pub offline: bool,
}

pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    let mut failures = 0usize;
    println!("toolchain");
    failures += check_toolchain();
    println!();
    println!("registries ({home})", home = home.display());
    failures += check_registries(home, args.offline).await;
    println!();
    println!("credentials");
    failures += check_credentials(home);
    println!();
    println!("installed extensions");
    failures += check_installed(home)?;
    println!();
    if failures > 0 {
        println!("{failures} problem(s) found");
        std::process::exit(1);
    }
    println!("all checks passed");
    Ok(())
}

fn check_toolchain() -> usize {
    // `cargo` is the only hard dependency (everything else is a build-time tool
    // the author installs on demand). Missing cargo-component / rustup /
    // wasm32-wasip2 target are warnings, not failures, so `gtdx doctor` on a
    // fresh machine exits 0 unless a real problem (bad describe, unreachable
    // registry, expired token) is present.
    let mut fails = 0;
    if let Ok(path) = which::which("cargo") {
        println!("  \u{2713} cargo  {}", path.display());
    } else {
        println!("  \u{2717} cargo not found — install Rust from https://rustup.rs/");
        fails += 1;
    }
    for (name, hint) in [
        ("cargo-component", "cargo install --locked cargo-component"),
        ("rustup", "install Rust from https://rustup.rs/"),
    ] {
        if let Ok(path) = which::which(name) {
            println!("  \u{2713} {name}  {}", path.display());
        } else {
            println!("  \u{26A0} {name} not found — {hint}");
        }
    }
    match std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            if s.lines().any(|l| l.trim() == "wasm32-wasip2") {
                println!("  \u{2713} wasm32-wasip2 target installed");
            } else {
                println!(
                    "  \u{26A0} wasm32-wasip2 target missing — rustup target add wasm32-wasip2"
                );
            }
        }
        _ => {
            println!("  \u{26A0} cannot list rustup targets");
        }
    }
    fails
}

async fn check_registries(home: &Path, offline: bool) -> usize {
    let cfg = match greentic_ext_registry::config::load(&home.join("config.toml")) {
        Ok(c) => c,
        Err(e) => {
            println!("  \u{26A0} cannot read config.toml: {e}");
            return 1;
        }
    };
    if cfg.registries.is_empty() {
        println!(
            "  \u{26A0} no registries configured — add one with: gtdx registries add <name> <url>"
        );
        return 0;
    }
    let mut fails = 0;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("reqwest client");
    for entry in &cfg.registries {
        if offline {
            println!(
                "  \u{25C9} {}  {}  (offline, not probed)",
                entry.name, entry.url
            );
            continue;
        }
        let health_url = format!("{}/health", entry.url.trim_end_matches('/'));
        match client.get(&health_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                println!("  \u{2713} {}  {}", entry.name, entry.url);
            }
            Ok(resp) => {
                println!(
                    "  \u{26A0} {}  {}  (health={} at {})",
                    entry.name,
                    entry.url,
                    resp.status(),
                    health_url
                );
            }
            Err(e) => {
                println!("  \u{2717} {}  {}  ({e})", entry.name, entry.url);
                fails += 1;
            }
        }
    }
    fails
}

fn check_credentials(home: &Path) -> usize {
    let path = home.join("credentials.toml");
    let creds = match Credentials::load(&path) {
        Ok(c) => c,
        Err(e) => {
            println!("  \u{26A0} cannot read credentials.toml: {e}");
            return 1;
        }
    };
    if creds.tokens.is_empty() {
        println!("  \u{25C9} no tokens stored — run gtdx login --registry <name> when needed");
        return 0;
    }
    let mut fails = 0;
    for (name, token) in &creds.tokens {
        match jwt_exp(token) {
            Some(exp) if exp > Utc::now() => {
                let dur = exp - Utc::now();
                println!("  \u{2713} {name}  expires in {}h", dur.num_hours());
            }
            Some(_) => {
                println!("  \u{2717} {name}  token expired — run: gtdx login --registry {name}");
                fails += 1;
            }
            None => {
                println!("  \u{25C9} {name}  non-JWT token (cannot verify expiry)");
            }
        }
    }
    fails
}

fn jwt_exp(token: &str) -> Option<DateTime<Utc>> {
    use base64::Engine as _;
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(payload))
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    let exp = v.get("exp")?.as_i64()?;
    DateTime::from_timestamp(exp, 0)
}

fn check_installed(home: &Path) -> anyhow::Result<usize> {
    let storage = Storage::new(home);
    let mut total = 0usize;
    let mut bad = 0usize;
    for kind in [
        ExtensionKind::Design,
        ExtensionKind::Bundle,
        ExtensionKind::Deploy,
    ] {
        let dir = storage.kind_dir(kind);
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            total += 1;
            let describe_path = entry.path().join("describe.json");
            if !describe_path.exists() {
                println!("  \u{2717} {} (no describe.json)", entry.path().display());
                bad += 1;
                continue;
            }
            let bytes = std::fs::read(&describe_path)?;
            let value: serde_json::Value = match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(e) => {
                    println!("  \u{2717} {}: invalid JSON: {e}", describe_path.display());
                    bad += 1;
                    continue;
                }
            };
            if let Err(e) = greentic_ext_contract::schema::validate_describe_json(&value) {
                println!("  \u{2717} {}: {e}", describe_path.display());
                bad += 1;
            } else {
                println!("  \u{2713} {}", describe_path.display());
            }
        }
    }
    if total == 0 {
        println!("  \u{25C9} no installed extensions");
    } else {
        println!("  {total} total, {bad} bad");
    }
    Ok(bad)
}
