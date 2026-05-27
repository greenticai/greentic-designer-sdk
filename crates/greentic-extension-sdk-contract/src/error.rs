use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("describe.json schema validation failed: {0}")]
    SchemaInvalid(String),

    #[error("capability id is malformed: {0}")]
    MalformedCapabilityId(String),

    #[error("component id is malformed: {0}")]
    MalformedComponentId(String),

    #[error("locale is malformed: {0}")]
    MalformedLocale(String),

    #[error("version is not semver: {0}")]
    MalformedVersion(String),

    #[error("sha256 is malformed: {0}")]
    MalformedSha256(String),

    #[error("signature verification failed: {0}")]
    SignatureInvalid(String),

    #[error("unsupported apiVersion: {0}")]
    UnsupportedApiVersion(String),

    #[error("canonicalization failed: {0}")]
    Canonicalize(String),

    /// Publisher certificate failed to parse or verify against the root.
    #[error("publisher cert invalid: {0}")]
    CertInvalid(String),

    /// The trust root is not available (e.g. production root key not yet
    /// provisioned — org-blocked).
    #[error("trust root unavailable: {0}")]
    TrustRootUnavailable(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
