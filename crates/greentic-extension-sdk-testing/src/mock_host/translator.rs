//! In-memory `i18n::t` / `i18n::tf` mock.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Returns canned translations keyed by `(locale, key)`. Falls back to the
/// key string itself if no translation is registered (matches real i18n
/// behavior on missing keys).
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::MockTranslator;
/// let t = MockTranslator::new();
/// t.set("id", "hello", "halo");
/// assert_eq!(t.translate("id", "hello"), "halo");
/// assert_eq!(t.translate("en", "hello"), "hello"); // fallback to key
/// assert_eq!(
///     t.translate_format("id", "hello", &[]),
///     "halo",
/// );
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockTranslator {
    inner: Arc<Mutex<HashMap<(String, String), String>>>,
}

impl MockTranslator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, locale: &str, key: &str, value: &str) {
        self.inner
            .lock()
            .expect("mock translator poisoned")
            .insert((locale.to_string(), key.to_string()), value.to_string());
    }

    #[must_use]
    pub fn translate(&self, locale: &str, key: &str) -> String {
        self.inner
            .lock()
            .expect("mock translator poisoned")
            .get(&(locale.to_string(), key.to_string()))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// `i18n::tf` analogue: token-substitute `{k}` in the looked-up value.
    #[must_use]
    pub fn translate_format(&self, locale: &str, key: &str, params: &[(&str, &str)]) -> String {
        let mut out = self.translate(locale, key);
        for (k, v) in params {
            out = out.replace(&format!("{{{k}}}"), v);
        }
        out
    }
}
