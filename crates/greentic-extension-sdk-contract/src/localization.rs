//! `LocalizedString` — either a plain `"foo"` string (deserialises with
//! empty `locales`) or `{ "default": "...", "locales": { ... } }`. Used by
//! `Metadata.summary`, `Metadata.description`, and `NodeType.label`.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ContractError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Locale(String);

impl Locale {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Locale {
    type Err = ContractError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(ContractError::MalformedLocale(s.into()));
        }
        if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(ContractError::MalformedLocale(s.into()));
        }
        Ok(Self(s.to_owned()))
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Locale {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// A possibly-localized string. Serialises as a plain JSON string when
/// `locales` is empty, and as `{default, locales}` otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizedString {
    default: String,
    locales: BTreeMap<Locale, String>,
}

impl LocalizedString {
    #[must_use]
    pub fn plain<S: Into<String>>(s: S) -> Self {
        Self {
            default: s.into(),
            locales: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_locales<S: Into<String>>(default: S, locales: BTreeMap<Locale, String>) -> Self {
        Self {
            default: default.into(),
            locales,
        }
    }

    #[must_use]
    pub fn default(&self) -> &str {
        &self.default
    }

    #[must_use]
    pub fn locales(&self) -> &BTreeMap<Locale, String> {
        &self.locales
    }

    #[must_use]
    pub fn lookup(&self, locale: &Locale) -> Option<&str> {
        self.locales.get(locale).map(String::as_str)
    }
}

impl Serialize for LocalizedString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.locales.is_empty() {
            serializer.serialize_str(&self.default)
        } else {
            #[derive(Serialize)]
            struct Obj<'a> {
                default: &'a str,
                locales: &'a BTreeMap<Locale, String>,
            }
            Obj {
                default: &self.default,
                locales: &self.locales,
            }
            .serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for LocalizedString {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Plain(String),
            Object {
                default: String,
                #[serde(default)]
                locales: BTreeMap<Locale, String>,
            },
        }
        Ok(match Raw::deserialize(d)? {
            Raw::Plain(s) => Self::plain(s),
            Raw::Object { default, locales } => Self::with_locales(default, locales),
        })
    }
}
