//! Minimal hex encoding for sha256 digests.
//!
//! Kept here (the root crate everyone depends on) to avoid duplicating the
//! trivial fold+write! loop across registry and testing.

use std::fmt::Write as _;

/// Encode a byte slice as a lowercase hexadecimal string.
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn encodes_empty_slice() {
        assert_eq!(encode(&[]), "");
    }

    #[test]
    fn encodes_zero_padded() {
        assert_eq!(encode(&[0x0a, 0xff, 0x00]), "0aff00");
    }
}
