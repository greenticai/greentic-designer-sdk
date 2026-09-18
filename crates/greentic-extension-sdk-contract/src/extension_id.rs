//! `metadata.id` validation — the one rule `gtdx new`, `gtdx lint` and
//! `gtdx publish` all share.
//!
//! There used to be four spellings of this rule and they disagreed, so an id
//! could be schema-valid and lint-clean yet unpublishable, or scaffoldable yet
//! lint-rejected, depending on which layer saw it first. This module is the
//! single source of truth; every caller reports the same verdict and quotes the
//! same [`EXTENSION_ID_PATTERN`].
//!
//! The rule: two or more `.`-separated segments, each a WIT kebab-name. No
//! namespace is privileged — `com.acme.my-ext` is as valid as `greentic.my-ext`.
//!
//! Segments are WIT kebab-names because `gtdx new` spends the id as one:
//! `id_to_wit_package` turns `greentic.telco-x` into `package greentic:telco-x;`
//! and `package.metadata.component.package`. 1.2.16 briefly allowed a segment
//! to start with a digit — `describe-v2.json` permits it — and that produced
//! scaffolds `cargo component build` refused with `invalid label: dash-separated
//! words must begin with an ASCII lowercase letter`. An id that cannot be built
//! is not a valid id, so the stricter rule wins.

use std::fmt;

use crate::wit_name::{KebabViolation, check_wit_kebab};

/// The regex the rule implements. Quoted verbatim in error messages, docs and
/// the `describe-v2.json` `metadata.id` description, so it must stay in step
/// with [`validate_extension_id`].
pub const EXTENSION_ID_PATTERN: &str =
    "^[a-z][a-z0-9]*(-[a-z][a-z0-9]*)*(\\.[a-z][a-z0-9]*(-[a-z][a-z0-9]*)*)+$";

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
    Segment {
        segment: String,
        violation: KebabViolation,
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
            Reason::Segment { segment, violation } => {
                write!(f, "in segment {segment:?}, {violation}")
            }
        }?;
        // The id becomes the WIT package name, so saying so here is what makes
        // a later `cargo component build` failure recognisable as this rule.
        write!(
            f,
            ". The id becomes the WIT package name, so it must match \
             {EXTENSION_ID_PATTERN}"
        )
    }
}

impl std::error::Error for ExtensionIdError {}

/// Validate a `metadata.id`, naming the offending segment and word when it
/// fails.
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
        match check_wit_kebab(segment) {
            Ok(()) => {}
            // An empty segment is about the id's dots, not the segment's
            // dashes, so it is reported in the id's own terms.
            Err(KebabViolation::Empty) => {
                return fail(Reason::EmptySegment {
                    position: position + 1,
                });
            }
            Err(violation) => {
                return fail(Reason::Segment {
                    segment: (*segment).to_owned(),
                    violation,
                });
            }
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
