//! File-system watcher wrapper + path filter.

use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use notify_debouncer_full::{
    DebounceEventResult, Debouncer, new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode},
};

/// Returns `true` when `path` (relative to the project root) is a file the dev
/// loop should rebuild on. Filters out `target/`, VCS metadata, editor swap
/// files, and OS droppings.
// Editors create lowercase swap/tmp suffixes; case-sensitive match is intentional.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub fn should_watch(path: &Path) -> bool {
    let comps: Vec<_> = path.components().collect();
    for c in &comps {
        let Some(s) = c.as_os_str().to_str() else {
            return false;
        };
        if matches!(
            s,
            "target" | ".git" | ".idea" | ".vscode" | "dist" | "node_modules" | ".DS_Store"
        ) {
            return false;
        }
    }
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if name.starts_with('~') || name.starts_with('.') && name.ends_with(".swp") {
        return false;
    }
    if name.ends_with(".swp") || name.ends_with(".tmp") || name.ends_with('~') {
        return false;
    }
    // Positive patterns: match any file under watched roots, or a known root-level file.
    let first = comps
        .first()
        .and_then(|c| c.as_os_str().to_str())
        .unwrap_or("");
    matches!(first, "src" | "wit" | "i18n" | "schemas" | "prompts")
        || matches!(name, "Cargo.toml" | "describe.json")
}

pub struct WatchHandle {
    _debouncer: Debouncer<RecommendedWatcher, notify_debouncer_full::RecommendedCache>,
    pub changes: mpsc::Receiver<Vec<PathBuf>>,
}

/// Spawn a recursive watcher rooted at `project_dir`. Every debounced batch is
/// filtered through `should_watch` and, if non-empty, sent as a `Vec<PathBuf>`
/// of project-relative paths.
pub fn spawn_watcher(project_dir: &Path, debounce: Duration) -> anyhow::Result<WatchHandle> {
    let (tx, rx) = mpsc::channel();
    let root = project_dir.to_path_buf();
    let debouncer = new_debouncer(debounce, None, move |res: DebounceEventResult| {
        let Ok(events) = res else {
            return;
        };
        let mut interesting = Vec::new();
        for ev in events {
            for path in &ev.paths {
                let Ok(rel) = path.strip_prefix(&root).map(Path::to_path_buf) else {
                    continue;
                };
                if should_watch(&rel) {
                    interesting.push(rel);
                }
            }
        }
        if !interesting.is_empty() {
            let _ = tx.send(interesting);
        }
    })?;
    let mut d = debouncer;
    d.watch(project_dir, RecursiveMode::Recursive)?;
    Ok(WatchHandle {
        _debouncer: d,
        changes: rx,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn watches_src_wit_describe_cargo() {
        assert!(should_watch(&p("src/lib.rs")));
        assert!(should_watch(&p("wit/world.wit")));
        assert!(should_watch(&p(
            "wit/deps/greentic/extension-base/world.wit"
        )));
        assert!(should_watch(&p("describe.json")));
        assert!(should_watch(&p("Cargo.toml")));
        assert!(should_watch(&p("i18n/en.json")));
        assert!(should_watch(&p("schemas/input.json")));
        assert!(should_watch(&p("prompts/system.md")));
    }

    #[test]
    fn ignores_target_git_ide_dirs() {
        assert!(!should_watch(&p("target/debug/foo.wasm")));
        assert!(!should_watch(&p(".git/HEAD")));
        assert!(!should_watch(&p(".idea/workspace.xml")));
        assert!(!should_watch(&p(".vscode/settings.json")));
        assert!(!should_watch(&p("dist/out.zip")));
        assert!(!should_watch(&p("node_modules/x/index.js")));
    }

    #[test]
    fn ignores_editor_swap_and_backup_files() {
        assert!(!should_watch(&p("src/.lib.rs.swp")));
        assert!(!should_watch(&p("src/lib.rs.tmp")));
        assert!(!should_watch(&p("src/lib.rs~")));
        assert!(!should_watch(&p("~backup.rs")));
    }

    #[test]
    fn ignores_out_of_scope_root_files() {
        assert!(!should_watch(&p("README.md")));
        assert!(!should_watch(&p("LICENSE")));
        assert!(!should_watch(&p("build.sh")));
    }

    #[test]
    fn spawn_watcher_delivers_batched_change_for_src_edit() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), b"// seed").unwrap();
        let handle = spawn_watcher(tmp.path(), Duration::from_millis(150)).expect("spawn");

        // Mutate after the watcher is up.
        std::thread::sleep(Duration::from_millis(50));
        std::fs::write(tmp.path().join("src/lib.rs"), b"// changed").unwrap();

        // Drain batches until we see the file path or timeout. On some
        // platforms (e.g. Linux inotify) the directory event lands first
        // and the file event arrives in a follow-up batch.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut saw_file = false;
        let mut all_paths: Vec<PathBuf> = Vec::new();
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match handle.changes.recv_timeout(remaining) {
                Ok(batch) => {
                    all_paths.extend(batch.iter().cloned());
                    if batch
                        .iter()
                        .any(|p| p == std::path::Path::new("src/lib.rs"))
                    {
                        saw_file = true;
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        assert!(
            saw_file,
            "expected src/lib.rs in a batch, got: {all_paths:?}"
        );
    }

    #[test]
    fn spawn_watcher_suppresses_target_churn() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("target")).unwrap();
        let handle = spawn_watcher(tmp.path(), Duration::from_millis(150)).expect("spawn");

        std::thread::sleep(Duration::from_millis(50));
        std::fs::write(tmp.path().join("target/out.bin"), b"x").unwrap();

        // No filtered event should arrive within the window.
        let res = handle.changes.recv_timeout(Duration::from_millis(750));
        assert!(res.is_err(), "target/ writes must not escape the filter");
    }
}
