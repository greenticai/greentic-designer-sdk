//! Writes `./dist/publish-<id>-<version>.json` receipts.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishReceiptJson {
    pub artifact: String,
    pub sha256: String,
    pub registry: String,
    pub published_at: DateTime<Utc>,
    pub trust_policy: String,
    pub signed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_known_limitations: Option<Vec<String>>,
}

pub fn receipt_path(dist_dir: &Path, ext_id: &str, version: &str) -> PathBuf {
    dist_dir.join(format!("publish-{ext_id}-{version}.json"))
}

pub fn write_receipt(
    dist_dir: &Path,
    ext_id: &str,
    version: &str,
    receipt: &PublishReceiptJson,
) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dist_dir)?;
    let path = receipt_path(dist_dir, ext_id, version);
    let bytes = serde_json::to_vec_pretty(receipt)?;
    std::fs::write(&path, bytes)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_path_includes_id_and_version() {
        let p = receipt_path(Path::new("/dist"), "com.example.demo", "0.1.0");
        assert_eq!(p, Path::new("/dist/publish-com.example.demo-0.1.0.json"));
    }

    #[test]
    fn write_receipt_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let receipt = PublishReceiptJson {
            artifact: "demo-0.1.0.gtxpack".into(),
            sha256: "abc".into(),
            registry: "file:///x".into(),
            published_at: Utc::now(),
            trust_policy: "loose".into(),
            signed: false,
            signing_known_limitations: None,
        };
        let path = write_receipt(tmp.path(), "com.example.demo", "0.1.0", &receipt).unwrap();
        assert!(path.exists());
        let read: PublishReceiptJson =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(read.artifact, "demo-0.1.0.gtxpack");
    }
}
