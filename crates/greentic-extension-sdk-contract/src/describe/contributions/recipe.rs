//! `Recipe` — packaging recipe contributed by a bundle extension.

use serde::{Deserialize, Serialize};

use crate::localization::LocalizedString;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Recipe {
    pub id: String,
    pub display_name: LocalizedString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<LocalizedString>,
    pub config_schema: String,
}
