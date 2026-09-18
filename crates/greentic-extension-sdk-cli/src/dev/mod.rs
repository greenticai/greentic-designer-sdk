//! Inner-loop dev command: rebuild -> pack -> install on source change.

pub mod builder;
pub mod event;
pub mod installer;
pub mod mount;
pub mod packer;
pub mod state;
pub mod watcher;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use tokio_util::sync::CancellationToken;

use self::builder::{Profile, run_build};
use self::event::{DevEvent, Emitter};
use self::installer::install_pack;
use self::packer::build_pack;
use self::watcher::spawn_watcher;

/// Rename the just-built `dist/dev.gtxpack` to its canonical
/// `<name>-<version>.gtxpack` display name. `info.ext_name` is free-form
/// `describe.json` metadata (an author can put a `/` in it, e.g.
/// "Topic / scope guardrail") and is sanitized via
/// [`greentic_extension_sdk_contract::safe_pack_filename`] before it becomes
/// a path component — an unsanitized `/` would otherwise be read as a path
/// separator and target a nonexistent nested directory.
fn rename_to_canonical(dist: &Path, info: packer::PackInfo) -> anyhow::Result<packer::PackInfo> {
    let pack_name =
        greentic_extension_sdk_contract::safe_pack_filename(&info.ext_name, &info.ext_version);
    let final_pack = dist.join(&pack_name);
    if final_pack == info.pack_path {
        return Ok(info);
    }
    if final_pack.exists() {
        std::fs::remove_file(&final_pack)
            .with_context(|| format!("remove stale {}", final_pack.display()))?;
    }
    std::fs::rename(&info.pack_path, &final_pack).with_context(|| {
        format!(
            "rename {} -> {}",
            info.pack_path.display(),
            final_pack.display()
        )
    })?;
    Ok(packer::PackInfo {
        pack_path: final_pack,
        pack_name,
        ..info
    })
}

/// Runtime parameters, resolved from `commands::dev::Args`.
#[derive(Debug, Clone)]
pub struct DevConfig {
    pub project_dir: PathBuf,
    pub home: PathBuf,
    pub profile: Profile,
    pub install: bool,
    pub debounce: Duration,
}

/// Resolve `Cargo.toml` path to the project root (its parent dir).
pub fn project_dir_from_manifest(manifest: &Path) -> anyhow::Result<PathBuf> {
    let canonical = std::fs::canonicalize(manifest)
        .map_err(|e| anyhow::anyhow!("canonicalize {}: {e}", manifest.display()))?;
    canonical
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow::anyhow!("manifest has no parent dir: {}", canonical.display()))
}

/// Perform a single build -> pack -> install cycle.
pub async fn run_once(cfg: &DevConfig, out: &mut dyn Emitter) -> anyhow::Result<()> {
    out.emit(&DevEvent::BuildStart {
        profile: cfg.profile.as_str().into(),
    });
    let build = match run_build(&cfg.project_dir, cfg.profile) {
        Ok(b) => b,
        Err(e) => {
            out.emit(&DevEvent::BuildFailed { duration_ms: 0 });
            return Err(e);
        }
    };
    out.emit(&DevEvent::BuildOk {
        duration_ms: build.duration_ms,
        wasm_size: build.wasm_size,
    });

    let dist = cfg.project_dir.join("dist");
    std::fs::create_dir_all(&dist)?;
    let out_pack = dist.join("dev.gtxpack");
    let info = build_pack(&cfg.project_dir, &build.wasm_path, &out_pack)?;
    let info = rename_to_canonical(&dist, info)?;
    out.emit(&DevEvent::PackOk {
        pack_name: info.pack_name.clone(),
        size: info.size,
    });

    if !cfg.install {
        out.emit(&DevEvent::InstallSkipped {
            reason: "--no-install".into(),
        });
        out.emit(&DevEvent::Idle {
            last_build_ok: true,
        });
        return Ok(());
    }

    match install_pack(&cfg.home, &info).await {
        Ok(summary) => {
            out.emit(&DevEvent::InstallOk {
                registry: summary.registry.display().to_string(),
                version: summary.version,
            });
            out.emit(&DevEvent::Idle {
                last_build_ok: true,
            });
            Ok(())
        }
        Err(e) => {
            out.emit(&DevEvent::Error {
                message: format!("install failed: {e}"),
            });
            out.emit(&DevEvent::Idle {
                last_build_ok: false,
            });
            Err(e)
        }
    }
}

/// Main watch loop: rebuild on every debounced FS batch, emit lifecycle events,
/// stay alive across build failures, exit cleanly on Ctrl+C.
pub async fn run_watch(cfg: &DevConfig, out: &mut dyn Emitter) -> anyhow::Result<()> {
    let cancel = CancellationToken::new();
    let cancel_signal = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        cancel_signal.cancel();
    });

    let handle = spawn_watcher(&cfg.project_dir, cfg.debounce)?;
    out.emit(&DevEvent::Ready {
        ext_id: probe_describe_id(&cfg.project_dir).unwrap_or_else(|| "unknown".into()),
        ext_version: probe_describe_version(&cfg.project_dir).unwrap_or_else(|| "unknown".into()),
        kind: probe_describe_kind(&cfg.project_dir).unwrap_or_else(|| "unknown".into()),
        registry: cfg.home.join("registries/dev-local").display().to_string(),
        watched_files: count_watched_files(&cfg.project_dir),
    });
    out.emit(&DevEvent::Idle {
        last_build_ok: true,
    });

    let mut last_pack_hash: Option<String> = None;
    let mut last_fingerprint = watched_fingerprint(&cfg.project_dir);

    loop {
        if cancel.is_cancelled() {
            out.emit(&DevEvent::Shutdown);
            return Ok(());
        }
        match handle.changes.recv_timeout(Duration::from_millis(250)) {
            Ok(batch) => {
                // An event is not a change. Skip silently when no watched byte
                // moved, or the build's own regenerated files re-trigger the
                // loop forever.
                let fingerprint = watched_fingerprint(&cfg.project_dir);
                if fingerprint == last_fingerprint {
                    tracing::debug!("watch event with no content change; skipping rebuild");
                    continue;
                }
                if let Some(p) = batch.first() {
                    out.emit(&DevEvent::ChangeDetected {
                        path: p.display().to_string(),
                    });
                }
                out.emit(&DevEvent::Debouncing {
                    window_ms: u64::try_from(cfg.debounce.as_millis()).unwrap_or(500),
                });
                if let Err(e) = run_once_cached(cfg, out, &mut last_pack_hash).await {
                    tracing::warn!("dev cycle failed: {e}");
                }
                // Re-read after the build: cargo-component rewrites bindings,
                // and the packer may fill digests into describe.json.
                last_fingerprint = watched_fingerprint(&cfg.project_dir);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                out.emit(&DevEvent::Error {
                    message: "watcher disconnected".into(),
                });
                return Err(anyhow::anyhow!("watcher channel closed"));
            }
        }
    }
}

async fn run_once_cached(
    cfg: &DevConfig,
    out: &mut dyn Emitter,
    last_pack_hash: &mut Option<String>,
) -> anyhow::Result<()> {
    out.emit(&DevEvent::BuildStart {
        profile: cfg.profile.as_str().into(),
    });
    let build = match run_build(&cfg.project_dir, cfg.profile) {
        Ok(b) => b,
        Err(e) => {
            out.emit(&DevEvent::BuildFailed { duration_ms: 0 });
            return Err(e);
        }
    };
    out.emit(&DevEvent::BuildOk {
        duration_ms: build.duration_ms,
        wasm_size: build.wasm_size,
    });

    let dist = cfg.project_dir.join("dist");
    std::fs::create_dir_all(&dist)?;
    let out_pack = dist.join("dev.gtxpack");
    let info = build_pack(&cfg.project_dir, &build.wasm_path, &out_pack)?;
    let info = rename_to_canonical(&dist, info)?;
    out.emit(&DevEvent::PackOk {
        pack_name: info.pack_name.clone(),
        size: info.size,
    });

    if !cfg.install {
        out.emit(&DevEvent::InstallSkipped {
            reason: "--no-install".into(),
        });
        out.emit(&DevEvent::Idle {
            last_build_ok: true,
        });
        return Ok(());
    }

    if last_pack_hash.as_deref() == Some(info.sha256.as_str()) {
        out.emit(&DevEvent::InstallSkipped {
            reason: "pack sha256 unchanged since last install".into(),
        });
        out.emit(&DevEvent::Idle {
            last_build_ok: true,
        });
        return Ok(());
    }

    match install_pack(&cfg.home, &info).await {
        Ok(summary) => {
            *last_pack_hash = Some(info.sha256);
            out.emit(&DevEvent::InstallOk {
                registry: summary.registry.display().to_string(),
                version: summary.version,
            });
            out.emit(&DevEvent::Idle {
                last_build_ok: true,
            });
            Ok(())
        }
        Err(e) => {
            out.emit(&DevEvent::Error {
                message: format!("install failed: {e}"),
            });
            out.emit(&DevEvent::Idle {
                last_build_ok: false,
            });
            Err(e)
        }
    }
}

/// Content fingerprint of every watched file, as `sha256(path + content-hash)`
/// folded over the set in a stable order.
///
/// The watcher fires on filesystem *events*, not on content changes, and a
/// `cargo component build` re-touches the very paths it is watching —
/// `src/bindings.rs`, `wit/deps/`, `Cargo.toml` — plus their parent
/// directories. Rebuilding on every event therefore feeds itself: on a fresh
/// scaffold the loop ran at roughly three builds a second, indefinitely, each
/// one reporting "pack sha256 unchanged" because nothing had actually changed.
///
/// Comparing this fingerprint instead answers the question the watcher cannot:
/// did any watched byte change? Recomputed *after* a build so the build's own
/// writes are folded into the baseline rather than triggering the next cycle.
fn watched_fingerprint(project_dir: &Path) -> String {
    let mut entries: Vec<(String, String)> = walkdir::WalkDir::new(project_dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let rel = e.path().strip_prefix(project_dir).ok()?.to_path_buf();
            if !watcher::should_watch(&rel) {
                return None;
            }
            let bytes = std::fs::read(e.path()).ok()?;
            Some((
                rel.to_string_lossy().replace('\\', "/"),
                greentic_extension_sdk_contract::sha256_hex(&bytes),
            ))
        })
        .collect();
    entries.sort();
    let joined = entries
        .into_iter()
        .map(|(path, hash)| format!("{path}:{hash}"))
        .collect::<Vec<_>>()
        .join("\n");
    greentic_extension_sdk_contract::sha256_hex(joined.as_bytes())
}

fn count_watched_files(project_dir: &Path) -> usize {
    walkdir::WalkDir::new(project_dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            e.path()
                .strip_prefix(project_dir)
                .ok()
                .map(Path::to_path_buf)
        })
        .filter(|p| watcher::should_watch(p))
        .count()
}

fn probe_describe_field(project_dir: &Path, key: &str) -> Option<String> {
    let describe_path = project_dir.join("describe.json");
    let bytes = match std::fs::read(&describe_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            // Surface the problem instead of silently showing "unknown" in the
            // dev loop banner — a missing/unreadable describe.json is a project
            // setup error the developer should fix.
            tracing::warn!(path = %describe_path.display(), error = %e, "cannot read describe.json");
            return None;
        }
    };
    let v: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(path = %describe_path.display(), error = %e, "describe.json is not valid JSON");
            return None;
        }
    };
    match key {
        "kind" => v["kind"].as_str().map(str::to_string),
        _ => v["metadata"][key].as_str().map(str::to_string),
    }
}

fn probe_describe_id(project_dir: &Path) -> Option<String> {
    probe_describe_field(project_dir, "id")
}

fn probe_describe_version(project_dir: &Path) -> Option<String> {
    probe_describe_field(project_dir, "version")
}

fn probe_describe_kind(project_dir: &Path) -> Option<String> {
    probe_describe_field(project_dir, "kind")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_info(pack_path: PathBuf, ext_name: &str) -> packer::PackInfo {
        packer::PackInfo {
            pack_path,
            pack_name: "dev.gtxpack".into(),
            size: 11,
            sha256: "dummy".into(),
            ext_name: ext_name.into(),
            ext_version: "0.1.0".into(),
            ext_kind: "design".into(),
            describe_bytes: Vec::new(),
        }
    }

    /// Empirical repro (`gtdx dev`/`gtdx dev --mount` variant): `ext_name`
    /// containing "/" (e.g. "Topic / scope guardrail") must not crash the
    /// rename to the canonical `<name>-<version>.gtxpack` display name —
    /// the raw name would otherwise be read as a path separator and target
    /// a nonexistent nested directory (`dist/Topic /...`).
    #[test]
    fn rename_to_canonical_sanitizes_slash_in_name() {
        let tmp = tempfile::tempdir().unwrap();
        let dist = tmp.path().join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        let out_pack = dist.join("dev.gtxpack");
        std::fs::write(&out_pack, b"pack-bytes").unwrap();

        let info = rename_to_canonical(&dist, pack_info(out_pack, "Topic / scope guardrail"))
            .expect("rename must succeed despite '/' in ext_name");

        assert!(info.pack_path.is_file());
        assert_eq!(info.pack_path.parent().unwrap(), dist);
        assert!(
            !info
                .pack_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains('/'),
            "canonical filename must not contain '/': {}",
            info.pack_path.display()
        );
        assert_eq!(std::fs::read(&info.pack_path).unwrap(), b"pack-bytes");
    }

    #[test]
    fn rename_to_canonical_leaves_safe_names_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let dist = tmp.path().join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        let out_pack = dist.join("dev.gtxpack");
        std::fs::write(&out_pack, b"pack-bytes").unwrap();

        let info = rename_to_canonical(&dist, pack_info(out_pack, "demo")).unwrap();

        assert_eq!(info.pack_path, dist.join("demo-0.1.0.gtxpack"));
        assert_eq!(info.pack_name, "demo-0.1.0.gtxpack");
    }

    /// The watcher fires on filesystem events, and a build re-touches the very
    /// paths it watches. Only a content change may start a rebuild — otherwise
    /// `gtdx dev` feeds itself forever.
    #[test]
    fn fingerprint_ignores_mtime_and_tracks_content() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), b"fn main() {}").unwrap();
        std::fs::write(root.join("describe.json"), b"{}").unwrap();

        let baseline = watched_fingerprint(root);

        // Rewriting identical bytes moves mtime but not content.
        std::fs::write(root.join("src/lib.rs"), b"fn main() {}").unwrap();
        assert_eq!(
            watched_fingerprint(root),
            baseline,
            "an mtime-only touch must not look like a change"
        );

        std::fs::write(root.join("src/lib.rs"), b"fn main() { changed() }").unwrap();
        assert_ne!(
            watched_fingerprint(root),
            baseline,
            "a real edit must be detected"
        );
    }

    /// Files the watcher ignores must not leak into the fingerprint, or every
    /// `cargo build` writing into `target/` would look like a source change.
    #[test]
    fn fingerprint_covers_only_watched_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::write(root.join("src/lib.rs"), b"fn main() {}").unwrap();

        let baseline = watched_fingerprint(root);
        std::fs::write(root.join("target/debug/out.wasm"), b"artifact").unwrap();

        assert_eq!(
            watched_fingerprint(root),
            baseline,
            "build output is not a source change"
        );
    }
}
