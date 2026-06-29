//! `Deprecated` — attached to `NodeType` and `CapabilityRef`. Designer renders
//! a warning chip in the palette; runner refuses to install if the current
//! contract version is past `removal_in`.

use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Deprecated {
    pub since: Version,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removal_in: Option<Version>,
}
