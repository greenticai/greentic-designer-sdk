//! Writer for .gtdx-contract.lock files.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ContractLock {
    pub contract_version: String,
    pub generated_by: String,
    pub generated_at: String,
    pub files: BTreeMap<String, String>,
}

impl ContractLock {
    pub fn to_toml(&self) -> anyhow::Result<String> {
        toml::to_string_pretty(self).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_serializes_with_files_table() {
        let mut files = BTreeMap::new();
        files.insert(
            "wit/deps/greentic/extension-base/world.wit".to_string(),
            "sha256:abc".to_string(),
        );
        let lock = ContractLock {
            contract_version: "0.1.0".to_string(),
            generated_by: "gtdx 0.1.0".to_string(),
            generated_at: "2026-04-18T00:00:00Z".to_string(),
            files,
        };
        let out = lock.to_toml().expect("serialize");
        assert!(out.contains("contract_version = \"0.1.0\""));
        assert!(out.contains("[files]"));
        assert!(out.contains("wit/deps/greentic/extension-base/world.wit"));
    }

    #[test]
    fn lock_file_ordering_is_deterministic() {
        let mut files = BTreeMap::new();
        files.insert("z.wit".to_string(), "z".to_string());
        files.insert("a.wit".to_string(), "a".to_string());
        let lock = ContractLock {
            contract_version: "0.1.0".to_string(),
            generated_by: "gtdx".to_string(),
            generated_at: "now".to_string(),
            files,
        };
        let out = lock.to_toml().unwrap();
        let a_pos = out.find("a.wit").unwrap();
        let z_pos = out.find("z.wit").unwrap();
        assert!(a_pos < z_pos, "files must be serialized in BTreeMap order");
    }
}
