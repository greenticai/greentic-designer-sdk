use std::fmt;
use std::str::FromStr;

use semver::VersionReq;
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::ContractError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CapabilityId(String);

impl<'de> Deserialize<'de> for CapabilityId {
    /// Route deserialization through [`FromStr`] so the validator actually
    /// runs, matching `ComponentId` and `Locale`.
    ///
    /// With the derived `transparent` impl this type read any `String`, and
    /// since every `CapabilityId` in production arrives via
    /// `serde_json::from_value::<DescribeJson>`, the validator below was dead
    /// code on the only path that mattered. `{"id": "no-colon-at-all"}` parsed,
    /// and `type_path()` then silently returned `""` — so capability matching
    /// failed far from the cause.
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

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
#[serde(deny_unknown_fields)]
pub struct CapabilityRef {
    pub id: CapabilityId,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<crate::deprecated::Deprecated>,
}

impl CapabilityRef {
    /// Parse the version requirement. Fails closed — a malformed string is an
    /// error, never a silent `*` match-everything (audit M2).
    ///
    /// # Errors
    /// `MalformedVersion` if `self.version` is not a valid semver requirement.
    pub fn version_req(&self) -> Result<VersionReq, ContractError> {
        VersionReq::parse(&self.version)
            .map_err(|e| ContractError::MalformedVersion(format!("{}: {e}", self.version)))
    }
}

#[cfg(test)]
mod deserialize_tests {
    use super::CapabilityId;

    /// The regression: the derived `transparent` impl skipped `FromStr`, so a
    /// malformed id parsed cleanly and only failed later, somewhere else.
    #[test]
    fn deserialize_rejects_what_from_str_rejects() {
        for bad in [
            "\"no-colon-at-all\"",
            "\"BAD NS!!:some/path\"",
            "\":path\"",
            "\"ns:\"",
        ] {
            assert!(
                serde_json::from_str::<CapabilityId>(bad).is_err(),
                "{bad} should not deserialize"
            );
        }
    }

    #[test]
    fn deserialize_accepts_a_well_formed_id() {
        let id: CapabilityId = serde_json::from_str("\"greentic:secret/prod-db\"").unwrap();
        assert_eq!(id.namespace(), "greentic");
        assert_eq!(id.type_path(), "secret/prod-db");
    }

    #[test]
    fn round_trips_through_json() {
        let id: CapabilityId = serde_json::from_str("\"greentic:perm/x\"").unwrap();
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"greentic:perm/x\"");
    }
}
