//! `Sha256` — typed wrapper for content-addressed digests. JSON form is a
//! 64-char lowercase hex string. Rust form is `[u8; 32]`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ContractError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256([u8; 32]);

impl Sha256 {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn as_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
        }
        s
    }
}

impl FromStr for Sha256 {
    type Err = ContractError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 64 {
            return Err(ContractError::MalformedSha256(format!(
                "expected 64 hex chars, got {}",
                s.len()
            )));
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            let pair = &s[i * 2..i * 2 + 2];
            let valid = pair
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
            if !valid {
                return Err(ContractError::MalformedSha256(format!(
                    "non-lowercase-hex char in {pair:?}"
                )));
            }
            bytes[i] = u8::from_str_radix(pair, 16)
                .map_err(|e| ContractError::MalformedSha256(e.to_string()))?;
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_hex())
    }
}

impl Serialize for Sha256 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_hex())
    }
}

impl<'de> Deserialize<'de> for Sha256 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}
