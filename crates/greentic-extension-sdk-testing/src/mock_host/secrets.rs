//! In-memory `secrets::get` mock.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// In-memory secrets store. Used by integration tests to feed an extension
/// a fake API key without touching the real keychain.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::MockSecretsBackend;
/// let s = MockSecretsBackend::new();
/// s.set("openai_key", "sk-test-123");
/// assert_eq!(s.get("openai_key").unwrap(), "sk-test-123");
/// assert!(s.get("missing").is_err());
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockSecretsBackend {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum MockSecretError {
    #[error("secret not found: {0}")]
    NotFound(String),
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

    /// # Errors
    ///
    /// Returns [`MockSecretError::NotFound`] when the alias is absent.
    pub fn get(&self, alias: &str) -> Result<String, MockSecretError> {
        self.inner
            .lock()
            .expect("mock secrets poisoned")
            .get(alias)
            .cloned()
            .ok_or_else(|| MockSecretError::NotFound(alias.to_string()))
    }
}
