//! `gtdx dev --mount <path>` — strict-parity one-shot mount mode.
//!
//! Build + pack + install the extension at `mount_dir` exactly the way
//! `gtdx install <pack>` would, with these caveats:
//!
//! 1. The install lands under `<home>/extensions/<kind>/<id>/` via the same
//!    `Installer::install` path the `dev --once` flow uses. This is strict
//!    parity — anything that breaks for a real install breaks here too,
//!    catching install-path bugs in the dev loop instead of in production.
//! 2. We use `TrustPolicy::Loose` (inherited from the existing `dev`
//!    installer) so an unsigned dev build still installs. Production
//!    runtimes trust this path only when the `dev-allow-unsigned` Cargo
//!    feature is enabled on `greentic-ext-runtime` (gated since D.2).
//!
//! Conflicts with `--watch` and `--once` at the CLI parser level — those
//! are loop modes; `--mount` is a one-shot.

use std::path::Path;

use crate::dev::builder::{Profile, run_build};
use crate::dev::installer::install_pack;
use crate::dev::packer::build_pack;

/// Build, pack, and install the extension at `mount_dir` once.
///
/// # Errors
///
/// Returns any error from `cargo component build`, the deterministic packer
/// (`build_gtxpack_with_manifest`), or the installer's filesystem-registry
/// staging + `Installer::install` flow.
pub async fn run_mount(mount_dir: &Path, home: &Path, release: bool) -> anyhow::Result<()> {
    let canonical = std::fs::canonicalize(mount_dir)
        .map_err(|e| anyhow::anyhow!("canonicalize {}: {e}", mount_dir.display()))?;
    let profile = if release {
        Profile::Release
    } else {
        Profile::Debug
    };

    let build = run_build(&canonical, profile)?;

    let dist = canonical.join("dist");
    std::fs::create_dir_all(&dist).map_err(|e| anyhow::anyhow!("mkdir {}: {e}", dist.display()))?;
    let out_pack = dist.join("dev.gtxpack");
    let info = build_pack(&canonical, &build.wasm_path, &out_pack)?;

    let summary = install_pack(home, &info).await?;
    eprintln!(
        "mounted {}@{} -> {}",
        info.ext_name,
        info.ext_version,
        summary.registry.display(),
    );
    Ok(())
}
