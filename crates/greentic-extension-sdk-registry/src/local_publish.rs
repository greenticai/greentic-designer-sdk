//! `LocalFilesystemRegistry::publish` implementation — hierarchical layout,
//! atomic temp-then-rename, advisory file lock on `index.json`.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::RegistryError;
use crate::local::LocalFilesystemRegistry;
use crate::publish::{PublishReceipt, PublishRequest};

const LOCK_FILE: &str = ".publish.lock";
const INDEX_FILE: &str = "index.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryIndex {
    pub extensions: Vec<IndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub versions: Vec<String>,
    pub latest: String,
}

impl LocalFilesystemRegistry {
    /// Publish `req.artifact_bytes` into `<root>/<id>/<version>/<name>-<version>.gtxpack`.
    /// Atomic: writes to `<target>.tmp`, fsyncs, renames. Concurrency: acquires
    /// an exclusive advisory lock on `<root>/.publish.lock` for the whole op.
    ///
    /// # Errors
    /// - `RegistryError::VersionExists` if version dir already present and `!req.force`.
    /// - `RegistryError::Io` for filesystem failures.
    pub fn publish_local(&self, req: &PublishRequest) -> Result<PublishReceipt, RegistryError> {
        let root = self.root_path();
        fs::create_dir_all(root)?;
        let _lock = acquire_lock(root)?;

        let ext_dir = root.join(&req.ext_id);
        let ver_dir = ext_dir.join(&req.version);

        if ver_dir.exists() && !req.force {
            let existing_sha = read_existing_sha(&ver_dir).unwrap_or_else(|_| "unknown".into());
            return Err(RegistryError::VersionExists { existing_sha });
        }
        if ver_dir.exists() && req.force {
            fs::remove_dir_all(&ver_dir)?;
        }
        fs::create_dir_all(&ver_dir)?;

        let pack_name = format!("{}-{}.gtxpack", req.ext_name, req.version);
        let pack_path = ver_dir.join(&pack_name);
        atomic_write(&pack_path, &req.artifact_bytes)?;

        let manifest_path = ver_dir.join("manifest.json");
        let manifest_bytes = serde_json::to_vec_pretty(&req.describe)?;
        atomic_write(&manifest_path, &manifest_bytes)?;

        if let Some(sig) = &req.signature {
            let sig_path = ver_dir.join("signature.json");
            let sig_bytes = serde_json::to_vec_pretty(sig)?;
            atomic_write(&sig_path, &sig_bytes)?;
        }

        let sha_sidecar = ver_dir.join("artifact.sha256");
        atomic_write(&sha_sidecar, req.artifact_sha256.as_bytes())?;

        update_index(root, req)?;
        update_metadata(&ext_dir, req)?;

        let url = format!("file://{}", pack_path.display());
        Ok(PublishReceipt {
            url,
            sha256: req.artifact_sha256.clone(),
            published_at: Utc::now(),
            signed: req.signature.is_some(),
        })
    }
}

fn acquire_lock(root: &Path) -> Result<File, RegistryError> {
    let lock_path = root.join(LOCK_FILE);
    let file = File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    file.lock_exclusive()
        .map_err(|e| RegistryError::Storage(format!("lock {}: {e}", lock_path.display())))?;
    Ok(file)
}

fn atomic_write(dest: &Path, bytes: &[u8]) -> Result<(), RegistryError> {
    let tmp = dest.with_extension(
        dest.extension()
            .map_or_else(|| "tmp".into(), |e| format!("{}.tmp", e.to_string_lossy())),
    );
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, dest)?;
    Ok(())
}

fn read_existing_sha(ver_dir: &Path) -> Result<String, RegistryError> {
    let path = ver_dir.join("artifact.sha256");
    Ok(fs::read_to_string(path)?.trim().to_string())
}

fn update_index(root: &Path, req: &PublishRequest) -> Result<(), RegistryError> {
    let index_path = root.join(INDEX_FILE);
    let mut index: RegistryIndex = if index_path.exists() {
        let bytes = fs::read(&index_path)?;
        serde_json::from_slice(&bytes).unwrap_or_default()
    } else {
        RegistryIndex::default()
    };

    let kind_str = serde_json::to_value(req.kind)?
        .as_str()
        .unwrap_or("Unknown")
        .to_string();

    if let Some(entry) = index.extensions.iter_mut().find(|e| e.id == req.ext_id) {
        if !entry.versions.contains(&req.version) {
            entry.versions.push(req.version.clone());
        }
        entry.versions.sort();
        entry.latest = entry.versions.last().cloned().unwrap_or_default();
        entry.name.clone_from(&req.ext_name);
        entry.kind = kind_str;
    } else {
        index.extensions.push(IndexEntry {
            id: req.ext_id.clone(),
            name: req.ext_name.clone(),
            kind: kind_str,
            versions: vec![req.version.clone()],
            latest: req.version.clone(),
        });
    }
    index.extensions.sort_by(|a, b| a.id.cmp(&b.id));

    atomic_write(&index_path, &serde_json::to_vec_pretty(&index)?)?;
    Ok(())
}

fn update_metadata(ext_dir: &Path, req: &PublishRequest) -> Result<(), RegistryError> {
    let path = ext_dir.join("metadata.json");
    let body = serde_json::to_vec_pretty(&req.describe)?;
    atomic_write(&path, &body)?;
    Ok(())
}
