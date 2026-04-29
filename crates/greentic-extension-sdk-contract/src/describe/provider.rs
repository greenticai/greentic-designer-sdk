//! Provider-specific extensions to the describe schema.
//!
//! `RuntimeGtpack` is an optional nested field on `Runtime` — populated when
//! `kind == ProviderExtension`. Enforces SHA-256 format at parse time.

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeGtpack {
    pub file: String,
    #[serde(deserialize_with = "deserialize_sha256")]
    pub sha256: String,
    pub pack_id: String,
    pub component_version: String,
}

fn deserialize_sha256<'de, D>(d: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(serde::de::Error::custom(format!(
            "invalid sha256: expected 64 lowercase hex chars, got len={} value={s:?}",
            s.len()
        )));
    }
    Ok(s)
}
