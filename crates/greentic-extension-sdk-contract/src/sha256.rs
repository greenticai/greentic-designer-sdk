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
        crate::hex::encode(&self.0)
    }
}

impl FromStr for Sha256 {
    type Err = ContractError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.as_bytes();
        if raw.len() != 64 {
            return Err(ContractError::MalformedSha256(format!(
                "expected 64 hex chars, got {}",
                raw.len()
            )));
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            let hi = hex_val(raw[i * 2])?;
            let lo = hex_val(raw[i * 2 + 1])?;
            bytes[i] = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }
}

fn hex_val(b: u8) -> Result<u8, ContractError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        other => Err(ContractError::MalformedSha256(format!(
            "non-lowercase-hex byte {other:#x}"
        ))),
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
