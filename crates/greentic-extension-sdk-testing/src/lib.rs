//! Test utilities for Greentic Designer Extensions.
//!
//! Builders for synthetic extensions and gtxpack ZIP helpers used across
//! the runtime and CLI test suites.

mod artifact;
mod fixture;
mod gtxpack;
mod node_type;
mod provider_fixtures;

pub use self::artifact::{assert_valid_artifact_output_json, fixture_generated_artifact};
pub use self::fixture::{ExtensionFixture, ExtensionFixtureBuilder};
pub use self::gtxpack::{pack_directory, unpack_to_dir};
pub use self::node_type::{
    assert_invalid_node_type_contributions, assert_valid_node_type_contributions,
    load_node_type_fixture,
};
pub use self::provider_fixtures::{
    build_provider_fixture_gtxpack, encode_gtpack_with_pack_id, sha256_hex,
};
