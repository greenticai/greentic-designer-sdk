use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use greentic_extension_sdk_contract::DescribeJson;

use crate::signing::load_signing_key;

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
    #[arg(long, default_value = crate::signing::DEFAULT_KEY_ENV)]
    pub key_env: String,
}

pub fn run(args: &Args, _home: &Path) -> Result<()> {
    let signing_key = load_signing_key(args.key.as_deref(), &args.key_env)?;

    let raw = std::fs::read_to_string(&args.describe_path)
        .with_context(|| format!("read {}", args.describe_path.display()))?;
    let mut describe: DescribeJson = serde_json::from_str(&raw).context("parse describe.json")?;

    greentic_extension_sdk_contract::sign_describe(&mut describe, &signing_key)
        .context("sign describe")?;

    let out = serde_json::to_string_pretty(&describe)? + "\n";
    std::fs::write(&args.describe_path, out)
        .with_context(|| format!("write {}", args.describe_path.display()))?;

    // sign_describe just populated the signature; report the key fingerprint
    // without an unwrap panic on the (now-present) signature (audit P3).
    if let Some(sig) = describe.signature.as_ref() {
        let pub_b64 = &sig.public_key;
        eprintln!(
            "signed {} with key {}",
            args.describe_path.display(),
            &pub_b64[..16.min(pub_b64.len())],
        );
    }
    Ok(())
}
