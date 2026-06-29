//! `Compat` — minimum designer/runner versions plus literal contract version
//! the descriptor was authored against. Parsed eagerly so invalid descriptors
//! fail at deserialize time, not when an installer tries to match.

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Compat {
    pub min_designer_version: VersionReq,
    pub min_runner_version: VersionReq,
    pub contract_version: Version,
}
