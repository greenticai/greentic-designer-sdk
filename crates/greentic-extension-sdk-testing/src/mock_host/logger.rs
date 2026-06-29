//! In-memory `Logger` mock that captures every log record.

use std::sync::{Arc, Mutex};

/// One captured log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub level: String,
    pub message: String,
}

/// Captures every `log(...)` call. Clone freely — the inner buffer is shared.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::MockLogger;
/// let logger = MockLogger::new();
/// logger.log("info", "hello");
/// logger.log("warn", "uh oh");
/// let records = logger.records();
/// assert_eq!(records.len(), 2);
/// assert_eq!(records[0].level, "info");
/// assert_eq!(records[1].message, "uh oh");
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockLogger {
    buf: Arc<Mutex<Vec<LogRecord>>>,
}

impl MockLogger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log(&self, level: &str, message: &str) {
        self.buf
            .lock()
            .expect("mocklogger poisoned")
            .push(LogRecord {
                level: level.to_string(),
                message: message.to_string(),
            });
    }

    #[must_use]
    pub fn records(&self) -> Vec<LogRecord> {
        self.buf.lock().expect("mocklogger poisoned").clone()
    }

    pub fn clear(&self) {
        self.buf.lock().expect("mocklogger poisoned").clear();
    }
}
