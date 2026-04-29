use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use greentic_ext_contract::DescribeJson;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to describe.json to sign in-place.
    pub describe_path: PathBuf,

    /// Read PKCS8 PEM private key from this file.
    /// Mutually exclusive with --key-env.
    #[arg(long, conflicts_with = "key_env")]
    pub key: Option<PathBuf>,

    /// Read PKCS8 PEM private key from this env var.
    /// Default: `GREENTIC_EXT_SIGNING_KEY_PEM`
    #[arg(long, default_value = "GREENTIC_EXT_SIGNING_KEY_PEM")]
    pub key_env: String,
}

pub fn run(args: &Args, _home: &Path) -> Result<()> {
    let pem = match &args.key {
        Some(path) => {
            std::fs::read_to_string(path).with_context(|| format!("read key {}", path.display()))?
        }
        None => std::env::var(&args.key_env).with_context(|| {
            format!(
                "env var ${} not set (use --key <path> or export the env var)",
                args.key_env
            )
        })?,
    };

    let signing_key = SigningKey::from_pkcs8_pem(&pem)
        .map_err(|e| anyhow::anyhow!("parse PKCS8 PEM private key: {e}"))?;

    let raw = std::fs::read_to_string(&args.describe_path)
        .with_context(|| format!("read {}", args.describe_path.display()))?;
    let mut describe: DescribeJson = serde_json::from_str(&raw).context("parse describe.json")?;

    greentic_ext_contract::sign_describe(&mut describe, &signing_key).context("sign describe")?;

    let out = serde_json::to_string_pretty(&describe)? + "\n";
    std::fs::write(&args.describe_path, out)
        .with_context(|| format!("write {}", args.describe_path.display()))?;

    let pub_b64 = &describe.signature.as_ref().unwrap().public_key;
    eprintln!(
        "signed {} with key {}",
        args.describe_path.display(),
        &pub_b64[..16.min(pub_b64.len())],
    );
    Ok(())
}
