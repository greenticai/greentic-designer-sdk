//! `metadata.id` validation — the one rule `gtdx new`, `gtdx lint` and
//! `gtdx publish` all share.
//!
//! There used to be four spellings of this rule, and they disagreed:
//! `describe-v2.json` and `gtdx lint` accepted a segment starting with a digit
//! while `gtdx new` and `gtdx publish` rejected it, so `gtdx new
//! 3aigent-designer` offered a default id its own wizard refused. `gtdx lint`
//! additionally hard-required the `greentic.` prefix, which no other layer
//! asked for. This module is the single source of truth; every caller reports
//! the same verdict and quotes the same [`EXTENSION_ID_PATTERN`].
//!
//! The rule: two or more `.`-separated segments; the first segment starts with
//! a lowercase letter (so an id never reads as a number); later segments may
//! start with a digit; every segment continues in lowercase letters, digits and
//! `-`. No namespace is privileged.

use std::fmt;

/// The regex the rule implements. Quoted verbatim in error messages, docs and
/// the `describe-v2.json` `metadata.id` description, so it must stay in step
/// with [`validate_extension_id`].
pub const EXTENSION_ID_PATTERN: &str = "^[a-z][a-z0-9-]*(\\.[a-z0-9][a-z0-9-]*)+$";

/// Why an id was rejected, with enough detail for the message to point at the
/// offending part rather than restating the regex.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Reason {
    Empty,
    SingleSegment,
    /// 1-based, counted the way an author reads the id.
    EmptySegment {
        position: usize,
    },
    FirstSegmentNotLetter {
        segment: String,
        first: char,
    },
    SegmentStartsWithHyphen {
        segment: String,
    },
    InvalidChar {
        segment: String,
        ch: char,
    },
}

/// An id that does not match [`EXTENSION_ID_PATTERN`], carrying the offending
/// id and the specific reason so [`fmt::Display`] can name both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionIdError {
    id: String,
    reason: Reason,
}

impl ExtensionIdError {
    /// The rejected id, verbatim.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl fmt::Display for ExtensionIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "extension id {:?} is invalid: ", self.id)?;
        match &self.reason {
            Reason::Empty => write!(f, "it is empty"),
            Reason::SingleSegment => write!(
                f,
                "it has no '.', but an id needs at least two dot-separated segments \
                 (a namespace and a name) — try \"greentic.{}\" or \"com.acme.{}\"",
                self.id, self.id
            ),
            Reason::EmptySegment { position } => write!(
                f,
                "segment {position} is empty — every '.' must sit between two segments, \
                 so no leading, trailing or doubled '.'"
            ),
            Reason::FirstSegmentNotLetter { segment, first } => write!(
                f,
                "the first segment {segment:?} starts with {first:?} — the first segment \
                 must start with a lowercase letter a-z (later segments may start with a \
                 digit, so \"greentic.3aigent\" is fine while \"3aigent.designer\" is not)"
            ),
            Reason::SegmentStartsWithHyphen { segment } => write!(
                f,
                "segment {segment:?} starts with '-' — a segment may contain '-' but must \
                 not begin with one"
            ),
            Reason::InvalidChar { segment, ch } if ch.is_whitespace() => write!(
                f,
                "segment {segment:?} contains whitespace — segments may only use lowercase \
                 letters a-z, digits 0-9 and '-'"
            ),
            Reason::InvalidChar { segment, ch } if ch.is_ascii_uppercase() => write!(
                f,
                "segment {segment:?} contains {ch:?} — segments may only use lowercase \
                 letters a-z, digits 0-9 and '-' (ids are lowercase-only)"
            ),
            Reason::InvalidChar { segment, ch } if *ch == '_' => write!(
                f,
                "segment {segment:?} contains {ch:?} — segments may only use lowercase \
                 letters a-z, digits 0-9 and '-' (use '-' instead of '_')"
            ),
            Reason::InvalidChar { segment, ch } => write!(
                f,
                "segment {segment:?} contains {ch:?} — segments may only use lowercase \
                 letters a-z, digits 0-9 and '-'"
            ),
        }?;
        write!(f, ". Expected {EXTENSION_ID_PATTERN}")
    }
}

impl std::error::Error for ExtensionIdError {}

/// Validate a `metadata.id`, naming the offending segment when it fails.
///
/// # Errors
///
/// Returns [`ExtensionIdError`] when `id` does not match
/// [`EXTENSION_ID_PATTERN`].
pub fn validate_extension_id(id: &str) -> Result<(), ExtensionIdError> {
    let fail = |reason| {
        Err(ExtensionIdError {
            id: id.to_owned(),
            reason,
        })
    };

    if id.is_empty() {
        return fail(Reason::Empty);
    }
    let segments: Vec<&str> = id.split('.').collect();
    if segments.len() < 2 {
        return fail(Reason::SingleSegment);
    }
    for (position, segment) in segments.iter().enumerate() {
        let mut chars = segment.chars();
        let Some(first) = chars.next() else {
            return fail(Reason::EmptySegment {
                position: position + 1,
            });
        };
        // Only the first segment is barred from starting with a digit: an id
        // whose very first character is a digit reads as a number, while
        // `greentic.3aigent-designer` does not.
        let first_ok = if position == 0 {
            first.is_ascii_lowercase()
        } else {
            first.is_ascii_lowercase() || first.is_ascii_digit()
        };
        if !first_ok {
            let segment = (*segment).to_owned();
            return if first == '-' {
                fail(Reason::SegmentStartsWithHyphen { segment })
            } else if position == 0 {
                fail(Reason::FirstSegmentNotLetter { segment, first })
            } else {
                fail(Reason::InvalidChar { segment, ch: first })
            };
        }
        if let Some(ch) =
            chars.find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
        {
            return fail(Reason::InvalidChar {
                segment: (*segment).to_owned(),
                ch,
            });
        }
    }
    Ok(())
}

/// Whether `id` matches [`EXTENSION_ID_PATTERN`]. Use
/// [`validate_extension_id`] when the caller can show the reason.
#[must_use]
pub fn is_valid_extension_id(id: &str) -> bool {
    validate_extension_id(id).is_ok()
}
