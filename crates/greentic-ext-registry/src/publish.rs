//! Types for `ExtensionRegistry::publish()` requests + receipts.

use chrono::{DateTime, Utc};
use greentic_ext_contract::{DescribeJson, ExtensionKind};
use serde::{Deserialize, Serialize};

/// One publish invocation: self-contained, backend-agnostic.
#[derive(Debug, Clone)]
pub struct PublishRequest {
    pub ext_id: String,
    pub ext_name: String,
    pub version: String,
    pub kind: ExtensionKind,
    pub artifact_bytes: Vec<u8>,
    pub artifact_sha256: String,
    pub describe: DescribeJson,
    pub signature: Option<SignatureBlob>,
    pub force: bool,
}

/// Optional signature carried alongside the artifact. The signature is over
/// the JCS-canonicalized describe.json (via `sign_describe`); Phase 1 does
/// NOT sign the artifact bytes themselves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureBlob {
    pub algorithm: String,
    pub public_key: String,
    pub value: String,
    pub key_id: String,
}

/// Confirmation returned from a successful publish.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishReceipt {
    pub url: String,
    pub sha256: String,
    pub published_at: DateTime<Utc>,
    pub signed: bool,
}
