//! Provider-specific extensions to the describe schema.
//!
//! `RuntimeGtpack` is an optional nested field on `Runtime` — populated when
//! `kind == ProviderExtension`. Enforces SHA-256 format at parse time.

use std::str::FromStr;

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
    // Reuse the canonical `Sha256` validator (lowercase-only). The previous
    // hand-rolled check used `is_ascii_hexdigit()`, which accepts uppercase
    // `A-F` — diverging from the v2 schema pattern `^[0-9a-f]{64}$`, the
    // `Sha256` newtype, and `pack_writer::sha256_hex`. That let a doc pass
    // deserialization yet fail schema validation (and vice-versa).
    crate::Sha256::from_str(&s).map_err(serde::de::Error::custom)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::RuntimeGtpack;

    fn json_with_sha(sha: &str) -> String {
        format!(
            r#"{{"file":"pack.gtpack","sha256":"{sha}","pack_id":"p","component_version":"1.0.0"}}"#
        )
    }

    #[test]
    fn accepts_lowercase_hex_sha256() {
        let sha = "a".repeat(64);
        let parsed: RuntimeGtpack = serde_json::from_str(&json_with_sha(&sha)).unwrap();
        assert_eq!(parsed.sha256, sha);
    }

    #[test]
    fn rejects_uppercase_hex_sha256() {
        let sha = "A".repeat(64);
        let err = serde_json::from_str::<RuntimeGtpack>(&json_with_sha(&sha)).unwrap_err();
        assert!(
            err.to_string().contains("non-lowercase-hex"),
            "expected lowercase-hex rejection, got: {err}"
        );
    }

    #[test]
    fn rejects_wrong_length_sha256() {
        let err = serde_json::from_str::<RuntimeGtpack>(&json_with_sha("abc")).unwrap_err();
        assert!(err.to_string().contains("64 hex chars"), "got: {err}");
    }
}
