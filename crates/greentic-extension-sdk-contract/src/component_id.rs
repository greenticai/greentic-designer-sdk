//! `ComponentId` — kebab-case identifier for a single WASM component contributed
//! by an extension. Used as the key in `Runtime.components` and as the value of
//! `NodeType.runtime_ref` / `Tool.runtime_ref`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::ContractError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ComponentId(String);

impl ComponentId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ComponentId {
    type Err = ContractError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(ContractError::MalformedComponentId(s.into()));
        }
        if !s.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' || c == '.'
        }) {
            return Err(ContractError::MalformedComponentId(s.into()));
        }
        Ok(Self(s.to_owned()))
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ComponentId {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}
