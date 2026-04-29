use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("extension not found: {name}@{version}")]
    NotFound { name: String, version: String },

    #[error("signature verification failed: {0}")]
    SignatureInvalid(String),

    #[error("auth required for {0}")]
    AuthRequired(String),

    #[error("auth failed: {0}")]
    AuthFailed(String),

    #[error("incompatible engine version: requires {required}, host provides {host}")]
    IncompatibleEngine { required: String, host: String },

    #[error("storage: {0}")]
    Storage(String),

    #[error("http: {0}")]
    Http(#[from] reqwest::Error),

    #[error("oci: {0}")]
    Oci(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("contract: {0}")]
    Contract(#[from] greentic_extension_sdk_contract::ContractError),

    #[error("provider install: {0}")]
    ProviderInstall(String),

    #[error("version already exists in registry (sha256={existing_sha})")]
    VersionExists { existing_sha: String },

    #[error("not implemented: {hint}")]
    NotImplemented { hint: String },
}
