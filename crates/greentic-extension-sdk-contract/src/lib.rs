//! Contract types + describe.json schema for Greentic Designer Extensions.

pub mod capability;
pub mod describe;
pub mod error;
pub mod hex;
pub mod kind;
pub mod pack_writer;
pub mod schema;
pub mod signature;

pub use self::capability::{CapabilityId, CapabilityRef, CapabilityVersion};
pub use self::describe::{DescribeJson, RuntimeGtpack};
pub use self::error::ContractError;
pub use self::kind::ExtensionKind;
pub use self::pack_writer::{PackEntry, PackWriterError, build_gtxpack, sha256_hex};
pub use self::signature::{
    artifact_sha256, canonical_signing_payload, sign_describe, sign_ed25519, verify_describe,
    verify_ed25519,
};
