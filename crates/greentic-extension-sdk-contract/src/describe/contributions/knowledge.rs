//! `Knowledge` — directory of knowledge-base artifacts shipped with the
//! extension. `path` is relative to the gtxpack root.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub path: String,
}
