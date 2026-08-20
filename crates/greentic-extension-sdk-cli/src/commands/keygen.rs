use std::io::IsTerminal as _;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine as _;
use clap::Args as ClapArgs;
use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::{EncodePrivateKey, spki::der::pem::LineEnding};
use rand::rngs::OsRng;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Write private key to this file instead of stdout (mode 0600).
    /// File must not already exist.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

pub fn run(args: &Args, _home: &Path) -> Result<()> {
    let signing_key = SigningKey::generate(&mut OsRng);
    let pem = signing_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| anyhow::anyhow!("encode PKCS8 PEM: {e}"))?;

    let pubkey_b64 =
        base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes());

    if let Some(path) = &args.out {
        write_mode_0600(path, pem.as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
        eprintln!("private key written: {}", path.display());
    } else {
        // Warn before writing an unencrypted private key to a terminal: it
        // lands in scroll-back, in shell history if piped, and in CI logs.
        // The `--out` path is the safe one (create_new + mode 0600).
        if std::io::stdout().is_terminal() {
            eprintln!(
                "warning: writing an unencrypted private key to your terminal. \
                 Use --out <file> instead — it is created with mode 0600."
            );
        }
        print!("{}", pem.as_str());
    }

    eprintln!("public key (base64): {pubkey_b64}");
    eprintln!();
    eprintln!("Next steps:");
    eprintln!("  1. Store the private key in your org vault (e.g., 1Password).");
    eprintln!("  2. Add as GH Actions secret: gh secret set EXT_SIGNING_KEY_PEM");
    eprintln!("  3. Distribute the public key via describe.json.signature.publicKey.");

    Ok(())
}

#[cfg(unix)]
fn write_mode_0600(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)
}

#[cfg(not(unix))]
fn write_mode_0600(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    f.write_all(bytes)
}
