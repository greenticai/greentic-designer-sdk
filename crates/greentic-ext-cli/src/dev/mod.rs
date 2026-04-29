//! Inner-loop dev command: rebuild -> pack -> install on source change.

pub mod builder;
pub mod event;
pub mod installer;
pub mod packer;
pub mod state;
pub mod watcher;

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use self::builder::{Profile, run_build};
use self::event::{DevEvent, Emitter};
use self::installer::install_pack;
use self::packer::build_pack;
use self::watcher::spawn_watcher;

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
    let final_pack = dist.join(format!("{}-{}.gtxpack", info.ext_name, info.ext_version));
    let info = if final_pack == info.pack_path {
        info
    } else {
        if final_pack.exists() {
            std::fs::remove_file(&final_pack)?;
        }
        std::fs::rename(&info.pack_path, &final_pack)?;
        packer::PackInfo {
            pack_path: final_pack,
            pack_name: format!("{}-{}.gtxpack", info.ext_name, info.ext_version),
            ..info
        }
    };
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

    loop {
        if cancel.is_cancelled() {
            out.emit(&DevEvent::Shutdown);
            return Ok(());
        }
        match handle.changes.recv_timeout(Duration::from_millis(250)) {
            Ok(batch) => {
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
    let final_pack = dist.join(format!("{}-{}.gtxpack", info.ext_name, info.ext_version));
    let info = if final_pack == info.pack_path {
        info
    } else {
        if final_pack.exists() {
            std::fs::remove_file(&final_pack)?;
        }
        std::fs::rename(&info.pack_path, &final_pack)?;
        packer::PackInfo {
            pack_path: final_pack,
            pack_name: format!("{}-{}.gtxpack", info.ext_name, info.ext_version),
            ..info
        }
    };
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
    let bytes = std::fs::read(project_dir.join("describe.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
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
