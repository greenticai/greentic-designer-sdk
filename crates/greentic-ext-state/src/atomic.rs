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

/// Write `content` atomically to `target` with an advisory file lock.
///
/// Locking strategy: a sibling `.lock` file gates concurrent writers.
/// Writers retry up to `MAX_LOCK_RETRIES` times with `LOCK_BACKOFF_MS`
/// delay, then return `StateError::LockContention`. The lock file is
/// released and removed on success.
///
/// Atomicity strategy: write to `<target>.tmp`, fsync, then rename over
/// `<target>`. Readers see either the old content or the new — never a
/// partial write.
pub(crate) fn write_atomic(target: &Path, content: &[u8]) -> Result<(), StateError> {
    let lock_path = lock_path_for(target);
    let lock_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&lock_path)?;

    acquire_lock(&lock_file)?;

    let tmp_path = target.with_extension("json.tmp");
    {
        let mut f = File::create(&tmp_path)?;
        f.write_all(content)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp_path, target)?;

    let _ = FileExt::unlock(&lock_file);
    drop(lock_file);
    let _ = std::fs::remove_file(&lock_path);
    Ok(())
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
