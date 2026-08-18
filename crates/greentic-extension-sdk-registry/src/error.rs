use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("extension not found: {name}@{version}")]
    NotFound { name: String, version: String },

    #[error("signature verification failed: {0}")]
    SignatureInvalid(String),

    #[error(
        "describe mismatch: the describe.json inside the artifact does not match the authenticated \
         describe served by the registry (a tampered registry may be trying to install broader \
         permissions than were consented to)"
    )]
    DescribeMismatch,

    #[error("artifact has no describe.json — cannot verify it matches the authenticated describe")]
    DescribeMissing,

    #[error(
        "registry served {served_name}@{served_version} for a request for \
         {requested_name}@{requested_version}; refusing install — the registry may be \
         substituting a different extension for the one you asked for"
    )]
    IdentityMismatch {
        requested_name: String,
        requested_version: String,
        served_name: String,
        served_version: String,
    },

    #[error(
        "artifact sha256 mismatch: registry advertised {expected}, downloaded bytes hash to {computed}"
    )]
    ArtifactHashMismatch { expected: String, computed: String },

    #[error("install of {name}@{version} declined: requested permissions were not granted")]
    PermissionDenied { name: String, version: String },

    #[error(
        "insecure registry url: {0} (https required; http allowed only for localhost/127.0.0.1)"
    )]
    InsecureRegistryUrl(String),

    #[error("artifact exceeds maximum size of {limit} bytes")]
    ArtifactTooLarge { limit: usize },

    #[error("{name}@{version} is yanked; re-run with --force to install anyway")]
    Yanked { name: String, version: String },

    #[error(
        "publisher key for {name} changed (pinned {pinned}, got {presented}); refusing install — \
         verify the publisher or remove the pin to re-trust"
    )]
    PublisherKeyChanged {
        name: String,
        pinned: String,
        presented: String,
    },

    #[error(
        "publisher of {name} is not trusted under Strict policy — its key is not in the trust store"
    )]
    UntrustedPublisher { name: String },

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
