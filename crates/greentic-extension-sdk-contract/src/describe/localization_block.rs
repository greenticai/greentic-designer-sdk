//! `Localization` — top-level translation table. Maps a flat string key
//! (e.g. `node.adaptive_card.label`) to a per-locale string map. Designer
//! reads this when a `LocalizedString` does not include inline locales.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::localization::Locale;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Localization {
    pub default_locale: Locale,
    pub strings: BTreeMap<String, BTreeMap<Locale, String>>,
}
