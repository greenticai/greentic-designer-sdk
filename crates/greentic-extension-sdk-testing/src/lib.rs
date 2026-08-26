//! Test utilities for Greentic Designer Extensions.
//!
//! Builders for synthetic extensions and gtxpack ZIP helpers used across
//! the runtime and CLI test suites.

#![forbid(unsafe_code)]

mod conformance;
mod fixture;
mod gtxpack;
pub mod mock_host;
mod provider_fixtures;

pub use self::conformance::{Inconsistency, assert_apply_consistent};
pub use self::fixture::{ExtensionFixture, ExtensionFixtureBuilder};
pub use self::gtxpack::{pack_directory, unpack_to_dir};
pub use self::provider_fixtures::{
    build_provider_fixture_gtxpack, encode_gtpack_with_pack_id, sha256_hex,
};
