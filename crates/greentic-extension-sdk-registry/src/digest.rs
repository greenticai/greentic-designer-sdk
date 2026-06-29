//! Shared artifact-digest helpers for the registry backends.
//!
//! Integrity verification of downloaded bytes happens in more than one place
//! (`store.rs` against the registry-advertised `artifact_sha256`, `oci.rs`
//! against the OCI manifest's layer descriptor). Both compare attacker-
//! influenced digest strings, so they share a single constant-time comparison
//! to avoid leaking — via response timing — how many leading characters of a
//! forged digest matched the real one.

use crate::error::RegistryError;

/// Compare two digest strings in constant time.
///
/// Returns `true` only when both strings are byte-for-byte equal. The
/// comparison cost does not short-circuit on the first differing byte, so it
/// does not reveal a partial match through timing.
#[must_use]
pub fn digests_equal(a: &str, b: &str) -> bool {
    constant_time_eq::constant_time_eq(a.as_bytes(), b.as_bytes())
}

/// Verify that `computed` matches `expected`, returning
/// [`RegistryError::ArtifactHashMismatch`] on a mismatch.
///
/// This is integrity, not authenticity: it proves the bytes were not truncated,
/// corrupted, or swapped relative to the digest the source advertised. It does
/// **not** establish publisher trust (that is signature verification).
pub fn verify_digest(expected: &str, computed: &str) -> Result<(), RegistryError> {
    if digests_equal(expected, computed) {
        Ok(())
    } else {
        Err(RegistryError::ArtifactHashMismatch {
            expected: expected.to_string(),
            computed: computed.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_digests_match() {
        assert!(digests_equal("sha256:abc", "sha256:abc"));
    }

    #[test]
    fn different_digests_do_not_match() {
        assert!(!digests_equal("sha256:abc", "sha256:abd"));
    }

    #[test]
    fn length_mismatch_does_not_match() {
        assert!(!digests_equal("sha256:abc", "sha256:abcd"));
    }

    #[test]
    fn verify_digest_ok_on_match() {
        verify_digest("deadbeef", "deadbeef").unwrap();
    }

    #[test]
    fn verify_digest_errs_on_mismatch() {
        let err = verify_digest("deadbeef", "cafebabe").unwrap_err();
        match err {
            RegistryError::ArtifactHashMismatch { expected, computed } => {
                assert_eq!(expected, "deadbeef");
                assert_eq!(computed, "cafebabe");
            }
            other => panic!("expected ArtifactHashMismatch, got {other}"),
        }
    }
}
