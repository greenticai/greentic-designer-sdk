//! Atomic state file writes with advisory file lock.

use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::StateError;

const MAX_LOCK_RETRIES: u32 = 3;
const LOCK_BACKOFF_MS: u64 = 50;

/// Run `f` while holding the exclusive advisory lock on `target`'s sibling
/// `.lock` file. The lock gates concurrent writers (and read-modify-write
/// cycles) so they serialize. Acquisition retries up to `MAX_LOCK_RETRIES`
/// times with `LOCK_BACKOFF_MS` delay, then returns `StateError::LockContention`.
///
/// The `.lock` file is created once and **left in place**. Removing it on
/// release is a classic flock anti-pattern: another process can be holding a
/// lock on the same inode, and deleting it lets a third process create a fresh
/// inode and "lock" it simultaneously — defeating mutual exclusion.
pub(crate) fn with_lock<T>(
    target: &Path,
    f: impl FnOnce() -> Result<T, StateError>,
) -> Result<T, StateError> {
    let lock_path = lock_path_for(target);
    // The lock file is only a lock handle; we never write content to it, so it
    // must not be truncated (that would needlessly churn the inode).
    let lock_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    acquire_lock(&lock_file)?;
    let result = f();
    let _ = FileExt::unlock(&lock_file);
    result
}

/// Write `content` to `target` via `tmp + fsync + rename` so readers see either
/// the old content or the new, never a partial write. The caller must already
/// hold the lock (see [`with_lock`]). On any error the temp file is removed so
/// a stale `.tmp` is never left behind.
pub(crate) fn write_file_atomic(target: &Path, content: &[u8]) -> Result<(), StateError> {
    let tmp_path = target.with_extension("json.tmp");
    let write = || -> Result<(), StateError> {
        let mut f = File::create(&tmp_path)?;
        f.write_all(content)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp_path, target)?;
        Ok(())
    };
    let result = write();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    result
}

/// Write `content` atomically to `target` while holding the write lock.
pub(crate) fn write_atomic(target: &Path, content: &[u8]) -> Result<(), StateError> {
    with_lock(target, || write_file_atomic(target, content))
}

fn acquire_lock(file: &File) -> Result<(), StateError> {
    for _ in 0..MAX_LOCK_RETRIES {
        if file.try_lock_exclusive().is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(LOCK_BACKOFF_MS));
    }
    Err(StateError::LockContention(MAX_LOCK_RETRIES))
}

fn lock_path_for(target: &Path) -> std::path::PathBuf {
    // <dir>/extensions-state.lock alongside <dir>/extensions-state.json
    let mut p = target.to_path_buf();
    p.set_extension("lock");
    p
}
