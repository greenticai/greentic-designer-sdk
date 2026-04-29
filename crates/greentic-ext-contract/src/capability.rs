use std::fmt;
use std::str::FromStr;

use semver::VersionReq;
use serde::{Deserialize, Serialize};

use crate::error::ContractError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityId(String);

impl CapabilityId {
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.0.split_once(':').map_or(&*self.0, |(ns, _)| ns)
    }

    #[must_use]
    pub fn type_path(&self) -> &str {
        self.0.split_once(':').map_or("", |(_, p)| p)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for CapabilityId {
    type Err = ContractError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ns, path) = s
            .split_once(':')
            .ok_or_else(|| ContractError::MalformedCapabilityId(s.into()))?;
        if ns.is_empty() || path.is_empty() {
            return Err(ContractError::MalformedCapabilityId(s.into()));
        }
        if !ns
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ContractError::MalformedCapabilityId(s.into()));
        }
        Ok(Self(s.to_owned()))
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub type CapabilityVersion = semver::Version;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRef {
    pub id: CapabilityId,
    pub version: String,
}

impl CapabilityRef {
    #[must_use]
    pub fn version_req(&self) -> VersionReq {
        VersionReq::parse(&self.version).unwrap_or(VersionReq::STAR)
    }
}
