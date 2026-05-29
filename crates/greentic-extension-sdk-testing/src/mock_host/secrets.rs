//! In-memory `secrets::get` mock.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// In-memory secrets store. Used by integration tests to feed an extension
/// a fake API key without touching the real keychain.
///
/// Optionally enforces the extension's declared secret permissions
/// (`runtime.permissions.secrets`): call [`restrict_to`](Self::restrict_to)
/// with the declared aliases and any `get` of an undeclared alias fails with
/// [`MockSecretError::PermissionDenied`] — mirroring the runtime's gate so a
/// test catches an extension reaching for a secret it never declared.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::MockSecretsBackend;
/// let s = MockSecretsBackend::new();
/// s.set("openai_key", "sk-test-123");
/// assert_eq!(s.get("openai_key").unwrap(), "sk-test-123");
/// assert!(s.get("missing").is_err());
///
/// // Enforce the declared permission set.
/// s.restrict_to(&["openai_key".to_string()]);
/// assert!(s.get("openai_key").is_ok());
/// s.set("other_key", "v");
/// assert!(s.get("other_key").is_err()); // declared nowhere → denied
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockSecretsBackend {
    inner: Arc<Mutex<HashMap<String, String>>>,
    /// `Some(allowlist)` enables permission enforcement; `None` allows any alias.
    allowed: Arc<Mutex<Option<Vec<String>>>>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum MockSecretError {
    #[error("secret not found: {0}")]
    NotFound(String),
    #[error("secret permission denied: {0} is not in the declared permissions.secrets")]
    PermissionDenied(String),
}

impl MockSecretsBackend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, alias: &str, value: &str) {
        self.inner
            .lock()
            .expect("mock secrets poisoned")
            .insert(alias.to_string(), value.to_string());
    }

    /// Enable permission enforcement against the declared secret aliases
    /// (typically `describe.runtime.permissions.secrets`). After this, `get` of
    /// any alias not in `aliases` fails with [`MockSecretError::PermissionDenied`].
    pub fn restrict_to(&self, aliases: &[String]) {
        *self.allowed.lock().expect("mock secrets poisoned") = Some(aliases.to_vec());
    }

    /// # Errors
    ///
    /// Returns [`MockSecretError::PermissionDenied`] when enforcement is enabled
    /// and `alias` is not declared, or [`MockSecretError::NotFound`] when the
    /// alias is permitted (or enforcement is off) but no value was set.
    pub fn get(&self, alias: &str) -> Result<String, MockSecretError> {
        if let Some(allowed) = self.allowed.lock().expect("mock secrets poisoned").as_ref()
            && !allowed.iter().any(|a| a == alias)
        {
            return Err(MockSecretError::PermissionDenied(alias.to_string()));
        }
        self.inner
            .lock()
            .expect("mock secrets poisoned")
            .get(alias)
            .cloned()
            .ok_or_else(|| MockSecretError::NotFound(alias.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restrict_to_distinguishes_denied_from_not_found() {
        let s = MockSecretsBackend::new();
        s.set("declared", "v");
        s.restrict_to(&["declared".to_string(), "also_declared".to_string()]);

        assert_eq!(s.get("declared").unwrap(), "v");
        // Declared but unset → NotFound (permitted, just no value).
        assert!(matches!(
            s.get("also_declared"),
            Err(MockSecretError::NotFound(_))
        ));
        // Undeclared → PermissionDenied, regardless of whether a value exists.
        s.set("sneaky", "v");
        assert!(matches!(
            s.get("sneaky"),
            Err(MockSecretError::PermissionDenied(_))
        ));
    }
}
