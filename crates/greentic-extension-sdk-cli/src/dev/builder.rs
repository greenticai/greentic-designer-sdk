//! `cargo component build` invocation + wasm artifact locator.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// Outcome of a single `cargo component build` invocation.
#[derive(Debug)]
pub struct BuildOutcome {
    pub wasm_path: PathBuf,
    pub duration_ms: u64,
    pub wasm_size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Debug,
    Release,
}

impl Profile {
    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Debug => "debug",
            Profile::Release => "release",
        }
    }
}

/// Build the `cargo component build` command (not yet executed).
pub fn cargo_command(project_dir: &Path, profile: Profile) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("component")
        .arg("build")
        .arg("--target")
        .arg("wasm32-wasip2")
        .current_dir(project_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if matches!(profile, Profile::Release) {
        cmd.arg("--release");
    }
    cmd
}

/// Run the build, streaming stdout/stderr to the caller's terminal. Returns the
/// emitted wasm path + duration + byte size on success.
pub fn run_build(project_dir: &Path, profile: Profile) -> anyhow::Result<BuildOutcome> {
    let start = Instant::now();
    let status = cargo_command(project_dir, profile).status()?;
    let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    if !status.success() {
        anyhow::bail!("cargo component build failed (exit {status})");
    }
    let wasm_path = find_wasm_artifact(project_dir, profile)?;
    let wasm_size = std::fs::metadata(&wasm_path)?.len();
    Ok(BuildOutcome {
        wasm_path,
        duration_ms,
        wasm_size,
    })
}

/// Resolve the cargo target directory for `project_dir`. Honors workspace
/// inheritance — for a workspace member, `cargo metadata` reports the
/// workspace-root target, which is where `cargo component build` actually
/// writes artifacts.
fn cargo_target_dir(project_dir: &Path) -> anyhow::Result<PathBuf> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version=1")
        .current_dir(project_dir)
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        // Fall back to <project_dir>/target. Standalone crates work fine that
        // way; workspace members will hit the bail! below if there's nothing.
        return Ok(project_dir.join("target"));
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let target = json
        .get("target_directory")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("cargo metadata missing target_directory"))?;
    Ok(PathBuf::from(target))
}

/// Locate the wasm component produced by the most recent build. Searches
/// `<target>/wasm32-wasip2/<profile>/` first, then
/// `<target>/wasm32-wasip1/<profile>/` (cargo-component 0.21 emits wasip2
/// components under the wasip1 directory because it compiles guests against
/// wasip1 and applies the p2 adapter at component-creation time).
///
/// `<target>` is resolved via `cargo metadata` so workspace members find
/// artifacts at the workspace root, not a non-existent crate-local
/// `target/`. Returns the first lexicographic `.wasm` so multi-target
/// workspaces get deterministic behavior.
pub fn find_wasm_artifact(project_dir: &Path, profile: Profile) -> anyhow::Result<PathBuf> {
    let profile_name = profile.as_str();
    let target_dir = cargo_target_dir(project_dir)?;
    let candidates_dirs = [
        target_dir.join("wasm32-wasip2").join(profile_name),
        target_dir.join("wasm32-wasip1").join(profile_name),
    ];
    for dir in &candidates_dirs {
        if !dir.exists() {
            continue;
        }
        let mut candidates: Vec<_> = std::fs::read_dir(dir)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("wasm"))
            .collect();
        candidates.sort();
        if let Some(first) = candidates.into_iter().next() {
            return Ok(first);
        }
    }
    anyhow::bail!(
        "no .wasm artifact under {}/wasm32-wasip2/{profile_name}/ or {}/wasm32-wasip1/{profile_name}/",
        target_dir.display(),
        target_dir.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_command_has_target_and_profile_flags() {
        let tmp = tempfile::tempdir().unwrap();
        let dbg = cargo_command(tmp.path(), Profile::Debug);
        let args: Vec<_> = dbg
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec!["component", "build", "--target", "wasm32-wasip2"]
        );

        let rel = cargo_command(tmp.path(), Profile::Release);
        let rel_args: Vec<_> = rel
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(rel_args.iter().any(|a| a == "--release"));
    }

    #[test]
    fn find_wasm_artifact_returns_first_lexicographic() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("target/wasm32-wasip2/debug");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("zeta.wasm"), b"z").unwrap();
        std::fs::write(dir.join("alpha.wasm"), b"a").unwrap();
        let got = find_wasm_artifact(tmp.path(), Profile::Debug).unwrap();
        assert_eq!(got.file_name().unwrap(), "alpha.wasm");
    }

    #[test]
    fn find_wasm_artifact_errors_when_dir_missing() {
        let tmp = tempfile::tempdir().unwrap();
        // cargo_target_dir falls back to <project_dir>/target when there's no
        // cargo manifest, so the error mentions that path.
        let err = find_wasm_artifact(tmp.path(), Profile::Debug).unwrap_err();
        assert!(err.to_string().contains("no .wasm"));
    }

    #[test]
    fn find_wasm_artifact_errors_when_no_wasm() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("target/wasm32-wasip2/release")).unwrap();
        let err = find_wasm_artifact(tmp.path(), Profile::Release).unwrap_err();
        assert!(err.to_string().contains("no .wasm"));
    }

    #[test]
    fn find_wasm_artifact_falls_back_to_wasip1_output_dir() {
        let tmp = tempfile::tempdir().unwrap();
        // cargo-component 0.21 lands wasip2 components here:
        let dir = tmp.path().join("target/wasm32-wasip1/debug");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("demo.wasm"), b"\0asm").unwrap();
        let got = find_wasm_artifact(tmp.path(), Profile::Debug).unwrap();
        assert_eq!(got.file_name().unwrap(), "demo.wasm");
    }
}
