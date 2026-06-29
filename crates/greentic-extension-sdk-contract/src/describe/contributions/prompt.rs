//! `Prompt` — markdown prompt fragment shipped with the extension. `path`
//! is relative to the gtxpack root.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Prompt {
    pub path: String,
}
