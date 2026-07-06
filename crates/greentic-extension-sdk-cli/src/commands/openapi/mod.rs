//! `gtdx openapi` — generate a `DesignExtension` connector from an `OpenAPI` 3.0 spec.
//!
//! This module currently provides the pure parse layer (`model::parse_openapi`),
//! which turns spec bytes into a [`model::ConnectorModel`]. Codegen (writing the
//! generated connector's Rust/WIT/describe.json files) is a follow-up task.

pub mod model;
