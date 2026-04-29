//! Test utilities for Greentic Designer Extensions.
//!
//! Builders for synthetic extensions and gtxpack ZIP helpers used across
//! the runtime and CLI test suites.

mod fixture;
mod gtxpack;
mod provider_fixtures;

pub use self::fixture::{ExtensionFixture, ExtensionFixtureBuilder};
pub use self::gtxpack::{pack_directory, unpack_to_dir};
pub use self::provider_fixtures::{
    build_provider_fixture_gtxpack, encode_gtpack_with_pack_id, sha256_hex,
};
