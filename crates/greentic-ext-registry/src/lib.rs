//! Registry client + install lifecycle for Greentic Designer Extensions.

pub mod config;
pub mod credentials;
pub mod error;
pub mod hex;
pub mod lifecycle;
pub mod local;
pub mod local_publish;
pub mod oci;
pub mod prompt;
pub mod provider_install;
pub mod publish;
pub mod registry;
pub mod storage;
pub mod store;
pub mod types;

pub use self::error::RegistryError;
pub use self::publish::{PublishReceipt, PublishRequest, SignatureBlob};
pub use self::registry::ExtensionRegistry;
pub use self::types::{
    ArtifactBytes, AuthToken, ExtensionArtifact, ExtensionMetadata, ExtensionSummary, SearchQuery,
};
