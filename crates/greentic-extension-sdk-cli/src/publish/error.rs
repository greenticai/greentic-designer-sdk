//! Typed publish error with spec §9 exit codes (split out of `mod.rs`).

/// Typed publish error with spec §9 exit codes.
#[derive(Debug)]
pub enum PublishError {
    /// describe.json missing, malformed, schema-invalid, or business-rule invalid. Exit 2.
    DescribeInvalid(String),
    /// `cargo component build` failed. Exit 70.
    Build(String),
    /// Target version already exists and `--force` was not supplied. Exit 10.
    VersionExists(String),
    /// Registry demands credentials but none were provided. Exit 20.
    AuthRequired(String),
    /// Registry refused the write (e.g. read-only / permissions). Exit 30.
    RegistryNotWritable(String),
    /// Backend path not yet implemented (Phase 2 stubs). Exit 50.
    NotImplemented(String),
    /// Filesystem I/O or network I/O failure. Exit 74.
    Io(String),
    /// Catch-all for unexpected errors. Exit 1.
    Other(anyhow::Error),
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PublishError::DescribeInvalid(m)
            | PublishError::Build(m)
            | PublishError::VersionExists(m)
            | PublishError::AuthRequired(m)
            | PublishError::RegistryNotWritable(m)
            | PublishError::NotImplemented(m)
            | PublishError::Io(m) => write!(f, "{m}"),
            PublishError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PublishError {}

impl PublishError {
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            PublishError::DescribeInvalid(_) => 2,
            PublishError::VersionExists(_) => 10,
            PublishError::AuthRequired(_) => 20,
            PublishError::RegistryNotWritable(_) => 30,
            PublishError::NotImplemented(_) => 50,
            PublishError::Build(_) => 70,
            PublishError::Io(_) => 74,
            PublishError::Other(_) => 1,
        }
    }
}

pub(super) fn io_err<E: std::fmt::Display>(e: E) -> PublishError {
    PublishError::Io(e.to_string())
}
