//! Preflight checks before scaffolding.

use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum Check {
    Pass { name: String, detail: String },
    Warn { name: String, hint: String },
    Fail { name: String, hint: String },
}

pub fn check_cargo_available() -> Check {
    match which::which("cargo") {
        Ok(path) => Check::Pass {
            name: "cargo".into(),
            detail: path.display().to_string(),
        },
        Err(_) => Check::Fail {
            name: "cargo".into(),
            hint: "install Rust toolchain from https://rustup.rs/".into(),
        },
    }
}

pub fn check_cargo_component_available() -> Check {
    match which::which("cargo-component") {
        Ok(path) => Check::Pass {
            name: "cargo-component".into(),
            detail: path.display().to_string(),
        },
        Err(_) => Check::Warn {
            name: "cargo-component".into(),
            hint: "install with: cargo install --locked cargo-component".into(),
        },
    }
}

pub fn check_target_dir(path: &Path, force: bool) -> Check {
    if !path.exists() {
        return Check::Pass {
            name: "target directory".into(),
            detail: format!("{} (will be created)", path.display()),
        };
    }
    match std::fs::read_dir(path) {
        Ok(mut entries) => {
            if entries.next().is_none() {
                Check::Pass {
                    name: "target directory".into(),
                    detail: format!("{} (empty)", path.display()),
                }
            } else if force {
                Check::Warn {
                    name: "target directory".into(),
                    hint: format!(
                        "{} is not empty; --force will overwrite existing files",
                        path.display()
                    ),
                }
            } else {
                Check::Fail {
                    name: "target directory".into(),
                    hint: format!(
                        "{} already exists and is not empty; pass --force to overwrite",
                        path.display()
                    ),
                }
            }
        }
        Err(e) => Check::Fail {
            name: "target directory".into(),
            hint: format!("cannot read {}: {}", path.display(), e),
        },
    }
}

pub fn check_wasm32_wasip2_target() -> Check {
    let output = std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let list = String::from_utf8_lossy(&o.stdout);
            if list.lines().any(|l| l.trim() == "wasm32-wasip2") {
                Check::Pass {
                    name: "wasm32-wasip2 target".into(),
                    detail: "installed".into(),
                }
            } else {
                Check::Warn {
                    name: "wasm32-wasip2 target".into(),
                    hint: "install with: rustup target add wasm32-wasip2".into(),
                }
            }
        }
        _ => Check::Warn {
            name: "wasm32-wasip2 target".into(),
            hint: "rustup not available; install wasm32-wasip2 manually before first build".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_check_is_pass_on_this_dev_env() {
        match check_cargo_available() {
            Check::Pass { .. } => {}
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn target_dir_missing_is_pass() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope");
        match check_target_dir(&missing, false) {
            Check::Pass { .. } => {}
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn target_dir_empty_is_pass() {
        let tmp = tempfile::tempdir().unwrap();
        match check_target_dir(tmp.path(), false) {
            Check::Pass { .. } => {}
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn target_dir_nonempty_no_force_is_fail() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("something"), "x").unwrap();
        match check_target_dir(tmp.path(), false) {
            Check::Fail { hint, .. } => {
                assert!(hint.contains("--force"));
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn target_dir_nonempty_with_force_is_warn() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("something"), "x").unwrap();
        match check_target_dir(tmp.path(), true) {
            Check::Warn { .. } => {}
            other => panic!("expected Warn, got {other:?}"),
        }
    }

    #[test]
    fn wasm_target_check_returns_some_variant() {
        let out = check_wasm32_wasip2_target();
        assert!(matches!(out, Check::Pass { .. } | Check::Warn { .. }));
    }
}
