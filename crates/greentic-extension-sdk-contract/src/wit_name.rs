//! WIT kebab-name checking — the Component Model's rule for a label.
//!
//! `gtdx new` turns `metadata.id` into the WIT package name
//! (`greentic.telco-x` -> `package greentic:telco-x;`), so every segment of an
//! id is spent as a WIT label. The Component Model requires each dash-separated
//! word to start with a lowercase ASCII letter; break it and
//! `cargo component build` fails with `invalid label: dash-separated words must
//! begin with an ASCII lowercase letter`, naming neither the id nor the file.
//!
//! [`check_wit_kebab`] returns a violation phrased about *a word*, with no
//! opinion about what the string as a whole is. Callers wrap it in a message
//! that names their own subject — an id segment, a project name.

use std::fmt;

/// The regex the rule implements: dash-separated words, each starting with a
/// lowercase ASCII letter and continuing in lowercase letters and digits.
pub const WIT_KEBAB_NAME_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z][a-z0-9]*)*$";

/// A specific way a string failed [`check_wit_kebab`], detailed enough for the
/// caller's message to point at the offending word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KebabViolation {
    /// The string is empty.
    Empty,
    /// A `-` sits at an edge or next to another `-`, so a word is empty.
    EmptyWord,
    /// A word starts with something other than `a`-`z`.
    WordNotLetterLed {
        /// The offending word, verbatim.
        word: String,
        /// Its first character.
        first: char,
    },
    /// A word contains a character outside `a`-`z` / `0`-`9`.
    InvalidChar {
        /// The offending word, verbatim.
        word: String,
        /// The offending character.
        ch: char,
    },
}

impl fmt::Display for KebabViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "it is empty"),
            Self::EmptyWord => write!(
                f,
                "it has an empty word — '-' must sit between two words, so no leading, \
                 trailing or doubled '-'"
            ),
            Self::WordNotLetterLed { word, first } if first.is_ascii_digit() => write!(
                f,
                "the word {word:?} starts with {first:?} — every dash-separated word must \
                 start with a lowercase letter a-z (a digit is fine after that, so \
                 \"aigent3\" works where \"3aigent\" does not)"
            ),
            Self::WordNotLetterLed { word, first } => write!(
                f,
                "the word {word:?} starts with {first:?} — every dash-separated word must \
                 start with a lowercase letter a-z"
            ),
            Self::InvalidChar { word, ch } if ch.is_whitespace() => write!(
                f,
                "the word {word:?} contains whitespace — only lowercase letters a-z, \
                 digits 0-9 and '-' are allowed"
            ),
            Self::InvalidChar { word, ch } if ch.is_ascii_uppercase() => write!(
                f,
                "the word {word:?} contains {ch:?} — only lowercase letters a-z, digits \
                 0-9 and '-' are allowed"
            ),
            Self::InvalidChar { word, ch } if *ch == '_' => write!(
                f,
                "the word {word:?} contains {ch:?} — only lowercase letters a-z, digits \
                 0-9 and '-' are allowed (use '-' instead of '_')"
            ),
            Self::InvalidChar { word, ch } => write!(
                f,
                "the word {word:?} contains {ch:?} — only lowercase letters a-z, digits \
                 0-9 and '-' are allowed"
            ),
        }
    }
}

impl std::error::Error for KebabViolation {}

/// Check one WIT label.
///
/// # Errors
///
/// Returns the first [`KebabViolation`] found, reading left to right.
pub fn check_wit_kebab(s: &str) -> Result<(), KebabViolation> {
    if s.is_empty() {
        return Err(KebabViolation::Empty);
    }
    for word in s.split('-') {
        let mut chars = word.chars();
        let Some(first) = chars.next() else {
            return Err(KebabViolation::EmptyWord);
        };
        if !first.is_ascii_lowercase() {
            // A digit-led word gets its own wording; anything else leading
            // (uppercase, `_`, `.`) reads better as the bad character it is.
            return Err(if first.is_ascii_digit() {
                KebabViolation::WordNotLetterLed {
                    word: word.to_owned(),
                    first,
                }
            } else {
                KebabViolation::InvalidChar {
                    word: word.to_owned(),
                    ch: first,
                }
            });
        }
        if let Some(ch) = chars.find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit())) {
            return Err(KebabViolation::InvalidChar {
                word: word.to_owned(),
                ch,
            });
        }
    }
    Ok(())
}

/// Whether `s` matches [`WIT_KEBAB_NAME_PATTERN`]. Use [`check_wit_kebab`] when
/// the caller can show the reason.
#[must_use]
pub fn is_valid_wit_kebab_name(s: &str) -> bool {
    check_wit_kebab(s).is_ok()
}
