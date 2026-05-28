//! Contract types + describe.json schema for Greentic Designer Extensions.

#![forbid(unsafe_code)]

pub mod agentic_worker;
pub mod capability;
pub mod compat;
pub mod component_id;
pub mod deprecated;
pub mod describe;
pub mod error;
pub mod hex;
pub mod kind;
pub mod localization;
pub mod manifest;
pub mod migration;
pub mod pack_writer;
pub mod publisher_cert;
pub mod root_verifier;
pub mod runtime_component;
pub mod schema;
pub mod sha256;
pub mod signature;

pub use self::agentic_worker::{
    AgenticWorkerMetadata, Cost, SideEffects, ToolCapability, UsageExample,
};
pub use self::capability::{CapabilityId, CapabilityRef, CapabilityVersion};
pub use self::compat::Compat;
pub use self::component_id::ComponentId;
pub use self::deprecated::Deprecated;
pub use self::describe::provider::RuntimeGtpack;
pub use self::describe::{DescribeJson, Signature, SignatureAlgorithm};
pub use self::error::ContractError;
pub use self::kind::ExtensionKind;
pub use self::localization::{Locale, LocalizedString};
pub use self::manifest::{
    DESCRIBE_ENTRY_NAME, MANIFEST_ENTRY_NAME, MANIFEST_SCHEMA_V1, MAX_ARCHIVE_BYTES,
    MAX_ENTRY_BYTES, Manifest, ManifestEntry, ManifestError, build_manifest,
    verify_archive_against_manifest,
};
pub use self::migration::{MigrationReport, migrate_v0_4_x_value};
pub use self::pack_writer::{
    PackEntry, PackWriterError, build_gtxpack, build_gtxpack_with_manifest, sha256_hex,
};
pub use self::publisher_cert::PublisherCert;
#[cfg(any(test, feature = "testing"))]
pub use self::root_verifier::FixtureRootVerifier;
pub use self::root_verifier::{EmbeddedRootVerifier, RootVerifier};
pub use self::runtime_component::RuntimeComponent;
pub use self::sha256::Sha256;
#[deprecated(
    note = "use verify_describe_with_key (authenticity) or verify_describe_self_consistent (integrity-only)"
)]
pub use self::signature::verify_describe_self_consistent as verify_describe;
pub use self::signature::{
    artifact_sha256, bind_manifest, canonical_signing_payload, sign_describe, sign_ed25519,
    verify_describe_self_consistent, verify_describe_with_key, verify_ed25519,
    verify_manifest_binding,
};
