//! Legacy v1 design+gtpack invariant tests. All ignored pending Phase A.2 migration.
//! Deleted in A.6.3.

#[test]
fn design_ext_with_gtpack_and_node_types_is_ok() {}

#[test]
fn design_ext_with_gtpack_but_no_node_types_is_err() {}

#[test]
fn design_ext_with_node_types_but_no_gtpack_is_ok() {}

#[test]
fn design_ext_with_empty_node_types_array_plus_gtpack_is_err() {}

#[test]
fn bundle_ext_with_gtpack_is_still_err() {}

#[test]
fn deploy_ext_with_gtpack_is_still_err() {}

#[test]
fn provider_ext_without_gtpack_is_still_err() {}
