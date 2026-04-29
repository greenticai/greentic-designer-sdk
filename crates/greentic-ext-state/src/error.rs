use std::io;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("parse: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("lock contention after {0} retries")]
    LockContention(u32),
}
