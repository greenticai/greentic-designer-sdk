use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;

use crate::dev::builder::Profile;
use crate::dev::project_dir_from_manifest;
use crate::publish::{PublishConfig, PublishOutcome, run_publish};

#[derive(ClapArgs, Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    /// Registry URI. `local` resolves to `$GREENTIC_HOME/registries/local`.
    /// Accepts `file://<path>`, `oci://<host>/<namespace>[/<artifact>]`, or a
    /// named entry from `~/.greentic/config.toml`.
    #[arg(short = 'r', long, default_value = "local")]
    pub registry: String,

    /// Override describe.json version for this run (CI version bumps).
    #[arg(long)]
    pub version: Option<String>,

    /// Build + pack + validate; skip registry write.
    #[arg(long)]
    pub dry_run: bool,

    /// Sign .gtxpack with local key from ~/.greentic/keys/.
    #[arg(long)]
    pub sign: bool,

    /// Signing key id (requires --sign).
    #[arg(long)]
    pub key_id: Option<String>,

    /// loose | normal | strict
    #[arg(long, default_value = "loose")]
    pub trust: String,

    /// Copy artifact here as well.
    #[arg(long, default_value = "./dist")]
    pub dist: PathBuf,

    /// Overwrite existing version.
    #[arg(long)]
    pub force: bool,

    /// cargo component build --release (default true for publish).
    #[arg(long, default_value_t = true)]
    pub release: bool,

    /// Skip build; only check registry for version conflict.
    #[arg(long)]
    pub verify_only: bool,

    /// Path to the project's Cargo.toml.
    #[arg(long, default_value = "./Cargo.toml")]
    pub manifest: PathBuf,

    /// human | json
    #[arg(long, default_value = "human")]
    pub format: String,

    /// Bearer token / PAT for `oci://...` registries. When omitted, falls
    /// back to env vars `GHCR_TOKEN`, `GITHUB_TOKEN`, `OCI_TOKEN` (in that
    /// order), then anonymous.
    #[arg(long)]
    pub oci_token: Option<String>,
}

pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    if args.sign {
        eprintln!(
            "warning: Phase 1 signing reuses Wave 1 JCS sign_describe. Safe to use, but key management + rotation land in Phase 2."
        );
    }
    let project_dir = project_dir_from_manifest(&args.manifest)?;
    let profile = if args.release {
        Profile::Release
    } else {
        Profile::Debug
    };
    let cfg = PublishConfig {
        project_dir,
        registry_uri: args.registry,
        home: home.to_path_buf(),
        dist_dir: args.dist,
        profile,
        dry_run: args.dry_run,
        force: args.force,
        sign: args.sign,
        key_id: args.key_id,
        version_override: args.version,
        trust_policy: args.trust,
        verify_only: args.verify_only,
        oci_token: args.oci_token,
    };
    match run_publish(&cfg).await {
        Ok(outcome) => {
            render_outcome(&args.format, &outcome)?;
            Ok(())
        }
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(err.exit_code());
        }
    }
}

fn render_outcome(format: &str, outcome: &PublishOutcome) -> anyhow::Result<()> {
    match format {
        "human" => {
            render_human(outcome);
            Ok(())
        }
        "json" => render_json(outcome),
        other => anyhow::bail!("unknown --format: {other} (use human|json)"),
    }
}

fn render_json(outcome: &PublishOutcome) -> anyhow::Result<()> {
    use serde_json::json;
    let v = match outcome {
        PublishOutcome::DryRun {
            artifact,
            sha256,
            registry,
        } => json!({
            "event": "dry_run",
            "artifact": artifact.display().to_string(),
            "sha256": sha256,
            "registry": registry,
        }),
        PublishOutcome::VerifyOnly {
            ext_id,
            version,
            registry,
        } => json!({
            "event": "verify_only",
            "ext_id": ext_id,
            "version": version,
            "registry": registry,
        }),
        PublishOutcome::Published {
            ext_id,
            version,
            sha256,
            artifact,
            receipt_path,
            signed,
            registry_url,
        } => json!({
            "event": "published",
            "ext_id": ext_id,
            "version": version,
            "sha256": sha256,
            "artifact": artifact.display().to_string(),
            "receipt_path": receipt_path.display().to_string(),
            "signed": signed,
            "registry_url": registry_url,
        }),
    };
    println!("{}", serde_json::to_string(&v)?);
    Ok(())
}

fn render_human(outcome: &PublishOutcome) {
    match outcome {
        PublishOutcome::DryRun {
            artifact,
            sha256,
            registry,
        } => {
            println!(
                "dry-run: would publish {} to {}",
                artifact.display(),
                registry
            );
            println!("sha256: {sha256}");
        }
        PublishOutcome::VerifyOnly {
            ext_id,
            version,
            registry,
        } => {
            println!("verify-only: {ext_id}@{version} slot free in {registry}");
        }
        PublishOutcome::Published {
            ext_id,
            version,
            sha256,
            artifact,
            receipt_path,
            signed,
            registry_url,
        } => {
            println!("\u{2713} published {ext_id}@{version}");
            println!("  artifact: {}", artifact.display());
            println!("  sha256:   {sha256}");
            println!("  registry: {registry_url}");
            println!("  signed:   {signed}");
            println!("  receipt:  {}", receipt_path.display());
        }
    }
}
