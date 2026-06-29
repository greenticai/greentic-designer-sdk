use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::RegistryError;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Credentials {
    #[serde(default)]
    pub tokens: std::collections::BTreeMap<String, String>,
}

// Hand-written so token values never leak via `{:?}`/tracing; the registry keys
// are kept for diagnostics (audit N10).
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field(
                "tokens",
                &self
                    .tokens
                    .keys()
                    .map(|k| (k.as_str(), "***"))
                    .collect::<std::collections::BTreeMap<_, _>>(),
            )
            .finish()
    }
}

impl Credentials {
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&s)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), RegistryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            // The credentials dir holds bearer tokens — keep it owner-only.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let s = toml::to_string_pretty(self)
            .map_err(|e| RegistryError::Storage(format!("toml ser: {e}")))?;

        #[cfg(unix)]
        {
            use std::io::Write as _;
            use std::os::unix::fs::OpenOptionsExt as _;
            use std::os::unix::fs::PermissionsExt as _;
            // Create the file 0o600 from the outset so a plaintext token is
            // never momentarily world-readable (the previous write-then-chmod
            // left a window at the default umask). `mode()` only applies on
            // creation, so also force 0o600 in case the file already existed.
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)?;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            file.write_all(s.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(path, s)?;
        }
        Ok(())
    }

    pub fn set(&mut self, registry: &str, token: &str) {
        self.tokens.insert(registry.into(), token.into());
    }

    #[must_use]
    pub fn get(&self, registry: &str) -> Option<&str> {
        self.tokens.get(registry).map(String::as_str)
    }

    pub fn remove(&mut self, registry: &str) -> Option<String> {
        self.tokens.remove(registry)
    }
}

#[cfg(test)]
mod debug_redaction_tests {
    use super::Credentials;

    #[test]
    fn debug_redacts_token_values() {
        // Credentials are sometimes captured in `{:?}`/tracing contexts; the
        // token values must never appear in the rendered output (audit N10).
        let mut creds = Credentials::default();
        creds.set("default", "super-secret-token");
        let rendered = format!("{creds:?}");
        assert!(
            !rendered.contains("super-secret-token"),
            "token value leaked into Debug: {rendered}"
        );
        assert!(
            rendered.contains("default"),
            "registry key should still be visible for diagnostics: {rendered}"
        );
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::Credentials;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn mode_of(path: &std::path::Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn save_restricts_file_and_parent_dir_to_owner_only() {
        let tmp = TempDir::new().unwrap();
        let cred_dir = tmp.path().join("gtdx");
        let path = cred_dir.join("credentials.toml");

        let mut creds = Credentials::default();
        creds.set("default", "secret-token");
        creds.save(&path).unwrap();

        assert_eq!(
            mode_of(&path),
            0o600,
            "credentials file must not be group/other-readable"
        );
        assert_eq!(
            mode_of(&cred_dir),
            0o700,
            "credentials dir must not be group/other-accessible"
        );
    }
}
