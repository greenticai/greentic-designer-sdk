//! In-memory `broker::call_extension` mock.
//!
//! Registers an extension id -> closure mapping so test code can simulate
//! cross-extension dispatch without standing up two real WASM components.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type Handler = Arc<dyn Fn(&str, &str) -> Result<String, String> + Send + Sync + 'static>;

/// A registry mapping `extension_id -> tool handler`.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::MockBroker;
/// use std::sync::Arc;
/// let b = MockBroker::new();
/// b.register("com.example.b", Arc::new(|tool, args| {
///     if tool == "echo" { Ok(args.to_string()) } else { Err("unknown".into()) }
/// }));
/// let out = b.call("com.example.b", "echo", "hi").unwrap();
/// assert_eq!(out, "hi");
/// ```
#[derive(Clone, Default)]
pub struct MockBroker {
    handlers: Arc<Mutex<HashMap<String, Handler>>>,
}

impl MockBroker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, ext_id: &str, handler: Handler) {
        self.handlers
            .lock()
            .expect("mock broker poisoned")
            .insert(ext_id.to_string(), handler);
    }

    /// # Errors
    ///
    /// Returns `Err(String)` when no handler is registered for `ext_id`, or
    /// when the registered handler itself returns an error.
    pub fn call(&self, ext_id: &str, tool: &str, args_json: &str) -> Result<String, String> {
        let handler = {
            let guard = self.handlers.lock().expect("mock broker poisoned");
            guard.get(ext_id).cloned()
        };
        match handler {
            Some(h) => h(tool, args_json),
            None => Err(format!("no mock extension registered for id {ext_id:?}")),
        }
    }
}

impl std::fmt::Debug for MockBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockBroker")
            .field(
                "registered",
                &self
                    .handlers
                    .lock()
                    .ok()
                    .map(|g| g.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default(),
            )
            .finish()
    }
}
