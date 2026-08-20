use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Args as ClapArgs;

use crate::dev::builder::Profile;
use crate::dev::event::{Format, StdoutEmitter};
use crate::dev::{DevConfig, project_dir_from_manifest, run_once, run_watch};

// CLI flags are naturally boolean; grouping into sub-structs would hurt UX.
#[allow(clippy::struct_excessive_bools)]
#[derive(ClapArgs, Debug, Clone)]
pub struct Args {
    /// Build + install once, then exit (CI-friendly)
    #[arg(long, conflicts_with = "watch")]
    pub once: bool,

    /// Continuous watch mode. This is the default; pass `--once` for a single
    /// build-pack-install pass.
    #[allow(dead_code)] // the default; `--once` is what actually branches
    #[arg(long)]
    pub watch: bool,

    /// Override target registry dir (currently informational; install uses home)
    // Reserved: consumed in future tracks. `hide` because advertising a flag
    // that does nothing is worse than not offering it — it still parses, so
    // existing scripts keep working.
    #[allow(dead_code)]
    #[arg(long, hide = true)]
    pub install_to: Option<PathBuf>,

    /// Build and pack only; skip installation
    #[arg(long)]
    pub no_install: bool,

    /// Build with `--release` (default: debug for speed)
    #[arg(long)]
    pub release: bool,

    /// Override `describe.json` id for this run (reserved for future multi-variant dev)
    // Reserved: consumed in future tracks; hidden until it is wired.
    #[allow(dead_code)]
    #[arg(long, hide = true)]
    pub ext_id: Option<String>,

    /// File-watch debounce window (ms)
    #[arg(long, default_value_t = default_debounce_ms())]
    pub debounce_ms: u64,

    /// Log filter level (not yet implemented — use `--verbose` or `RUST_LOG`)
    // Reserved: consumed in future tracks. This one was the most misleading of
    // the four: it advertised `[default: info]`, so it read as the way to
    // control verbosity, while logging came only from RUST_LOG.
    #[allow(dead_code)]
    #[arg(long, default_value = "info", hide = true)]
    pub log: String,

    /// Path to the project's `Cargo.toml`
    #[arg(long, default_value = "./Cargo.toml")]
    pub manifest: PathBuf,

    /// Mount mode: build + pack + install the extension at `<path>` once,
    /// exactly the way `gtdx install` would. Conflicts with `--watch` and
    /// `--once`. Intended for dev-loop "drop a built extension into a fresh
    /// runtime" iterations without the file watcher.
    #[arg(long, conflicts_with_all = ["watch", "once"])]
    pub mount: Option<PathBuf>,

    /// Force a full rebuild by running `cargo clean -p <crate>` first
    // Reserved: consumed in future tracks; hidden until it is wired.
    #[allow(dead_code)]
    #[arg(long, hide = true)]
    pub force_rebuild: bool,

    /// Output format: human | json
    #[arg(long, default_value = "human")]
    pub format: String,
}

fn default_debounce_ms() -> u64 {
    if cfg!(windows) { 1000 } else { 500 }
}

pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    if let Some(mount_dir) = &args.mount {
        return crate::dev::mount::run_mount(mount_dir, home, args.release).await;
    }
    let project_dir = project_dir_from_manifest(&args.manifest)?;
    let profile = if args.release {
        Profile::Release
    } else {
        Profile::Debug
    };
    let cfg = DevConfig {
        project_dir,
        home: home.to_path_buf(),
        profile,
        install: !args.no_install,
        debounce: Duration::from_millis(args.debounce_ms),
    };
    let format = Format::parse(&args.format)?;
    let mut emitter = StdoutEmitter { format };

    if args.once {
        run_once(&cfg, &mut emitter).await
    } else {
        run_watch(&cfg, &mut emitter).await
    }
}
