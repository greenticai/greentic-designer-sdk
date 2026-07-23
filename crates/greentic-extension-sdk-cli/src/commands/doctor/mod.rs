pub mod designer_compat;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use clap::Args as ClapArgs;
use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::credentials::Credentials;
use greentic_extension_sdk_registry::storage::Storage;

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
    println!("designer compatibility");
    failures += check_designer_compat(home)?;
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
    let cfg = match greentic_extension_sdk_registry::config::load(&home.join("config.toml")) {
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
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            // `doctor` is the command users run on a broken machine — a TLS/HTTP
            // backend init failure must be reported, not panic the diagnostics.
            println!("  \u{2717} cannot build HTTP client to probe registries: {e}");
            return cfg.registries.len();
        }
    };
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

/// Every installed extension directory across all kinds, in a stable order.
///
/// Shared by the describe-validity check and the designer-compatibility check
/// so the two always report on the same set of extensions.
fn installed_extension_dirs(home: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let storage = Storage::new(home);
    let mut dirs = Vec::new();
    for kind in [
        ExtensionKind::Design,
        ExtensionKind::Bundle,
        ExtensionKind::Deploy,
        ExtensionKind::WasixMcpRouter,
    ] {
        let kind_dir = storage.kind_dir(kind);
        if !kind_dir.exists() {
            continue;
        }
        let mut of_kind = Vec::new();
        for entry in std::fs::read_dir(&kind_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                of_kind.push(entry.path());
            }
        }
        of_kind.sort();
        dirs.extend(of_kind);
    }
    Ok(dirs)
}

fn check_installed(home: &Path) -> anyhow::Result<usize> {
    let dirs = installed_extension_dirs(home)?;
    let total = dirs.len();
    let mut bad = 0usize;
    for ext_dir in dirs {
        let describe_path = ext_dir.join("describe.json");
        if !describe_path.exists() {
            println!("  \u{2717} {} (no describe.json)", ext_dir.display());
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
        if let Err(e) = greentic_extension_sdk_contract::schema::validate_describe_json(&value) {
            println!("  \u{2717} {}: {e}", describe_path.display());
            bad += 1;
        } else {
            println!("  \u{2713} {}", describe_path.display());
        }
    }
    if total == 0 {
        println!("  \u{25C9} no installed extensions");
    } else {
        println!("  {total} total, {bad} bad");
    }
    Ok(bad)
}

/// Path to the designer binary doctor should interrogate.
///
/// `GREENTIC_DESIGNER_BIN` takes priority so an author running designer out of
/// a checkout (`target/release/greentic-designer`) can point doctor at the
/// build they actually launch, which is usually not the one on `PATH`.
fn designer_binary() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("GREENTIC_DESIGNER_BIN") {
        let path = PathBuf::from(explicit);
        return path.exists().then_some(path);
    }
    which::which("greentic-designer").ok()
}

fn designer_version(binary: &Path) -> Option<semver::Version> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    designer_compat::parse_version_output(&String::from_utf8_lossy(&output.stdout))
}

/// Human label for an extension directory: `<id> <version>` from the describe,
/// falling back to the directory name.
fn extension_label(ext_dir: &Path, describe: &serde_json::Value) -> String {
    let metadata = describe.get("metadata");
    let id = metadata
        .and_then(|m| m.get("id"))
        .and_then(serde_json::Value::as_str);
    let version = metadata
        .and_then(|m| m.get("version"))
        .and_then(serde_json::Value::as_str);
    match (id, version) {
        (Some(id), Some(version)) => format!("{id} {version}"),
        (Some(id), None) => id.to_string(),
        _ => ext_dir.file_name().map_or_else(
            || ext_dir.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ),
    }
}

/// Report which installed extensions the local designer build can actually
/// load. An extension built against the current SDK emits a `greentic.ai/v2`
/// describe, which a pre-1.2 designer skips at boot with no actionable hint —
/// so it never appears in `/api/extensions` and the author has nothing to go
/// on. Naming the mismatch here is the whole point of the check.
fn check_designer_compat(home: &Path) -> anyhow::Result<usize> {
    let Some(binary) = designer_binary() else {
        println!(
            "  \u{25C9} greentic-designer not found on PATH — skipping \
             (set GREENTIC_DESIGNER_BIN to check a build from a checkout)"
        );
        return Ok(0);
    };
    let Some(version) = designer_version(&binary) else {
        println!(
            "  \u{26A0} cannot read a version from {} --version — skipping",
            binary.display()
        );
        return Ok(0);
    };
    println!(
        "  \u{2713} greentic-designer {version}  {}",
        binary.display()
    );

    let dirs = installed_extension_dirs(home)?;
    if dirs.is_empty() {
        println!("  \u{25C9} no installed extensions to check");
        return Ok(0);
    }

    let mut problems = 0usize;
    for ext_dir in dirs {
        let Ok(bytes) = std::fs::read(ext_dir.join("describe.json")) else {
            // Already reported by `check_installed`; not a compat problem.
            continue;
        };
        let Ok(describe) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        let label = extension_label(&ext_dir, &describe);
        let verdict = designer_compat::evaluate(&version, &describe);
        match verdict.remedy(&version) {
            Some(remedy) => {
                println!("  \u{2717} {label}: {remedy}");
                problems += 1;
            }
            None => println!("  \u{2713} {label}"),
        }
    }
    Ok(problems)
}
