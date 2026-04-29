use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("describe.json schema validation failed: {0}")]
    SchemaInvalid(String),

    #[error("capability id is malformed: {0}")]
    MalformedCapabilityId(String),

    #[error("version is not semver: {0}")]
    MalformedVersion(String),

    #[error("signature verification failed: {0}")]
    SignatureInvalid(String),

    #[error("unsupported apiVersion: {0}")]
    UnsupportedApiVersion(String),

    #[error("canonicalization failed: {0}")]
    Canonicalize(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
