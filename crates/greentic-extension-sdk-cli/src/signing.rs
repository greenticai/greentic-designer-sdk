//! Shared ed25519 signing-key loader.
//!
//! One canonical key format across the CLI: **PKCS8 PEM**, exactly what
//! `gtdx keygen` emits. `gtdx sign` and `gtdx publish --sign` both resolve their
//! signing key through this module so a key produced by `keygen` works verbatim
//! in either command (audit H5 — `publish` previously read a base64 raw seed,
//! which `keygen` never produces).

use std::path::Path;

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::DecodePrivateKey;

/// Default env var holding a PKCS8 PEM signing key (CI / headless signing).
pub const DEFAULT_KEY_ENV: &str = "GREENTIC_EXT_SIGNING_KEY_PEM";

/// Parse an ed25519 PKCS8 PEM private key from its textual contents.
///
/// # Errors
/// Returns an error if `pem` is not a valid ed25519 PKCS8 PEM document.
pub fn signing_key_from_pem(pem: &str) -> Result<SigningKey> {
    SigningKey::from_pkcs8_pem(pem).map_err(|e| anyhow::anyhow!("parse PKCS8 PEM private key: {e}"))
}

/// Load an ed25519 PKCS8 PEM signing key from an explicit `path`, falling back
/// to the `env_var` environment variable when `path` is `None`.
///
/// # Errors
/// Returns an error if the file cannot be read, the env var is unset, or the
/// loaded bytes are not a valid ed25519 PKCS8 PEM key.
pub fn load_signing_key(path: Option<&Path>, env_var: &str) -> Result<SigningKey> {
    let pem = match path {
        Some(p) => std::fs::read_to_string(p)
            .with_context(|| format!("read signing key {}", p.display()))?,
        None => std::env::var(env_var).with_context(|| {
            format!("env var ${env_var} not set (use --key <path> or export the env var)")
        })?,
    };
    signing_key_from_pem(&pem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::pkcs8::{EncodePrivateKey, spki::der::pem::LineEnding};

    /// A deterministic PKCS8 PEM identical in shape to what `gtdx keygen` writes.
    fn sample_pem() -> String {
        SigningKey::from_bytes(&[7u8; 32])
            .to_pkcs8_pem(LineEnding::LF)
            .unwrap()
            .to_string()
    }

    #[test]
    fn parses_keygen_style_pkcs8_pem() {
        let key = signing_key_from_pem(&sample_pem()).unwrap();
        assert_eq!(key.to_bytes(), [7u8; 32]);
    }

    #[test]
    fn rejects_base64_raw_seed() {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        let raw_seed_b64 = B64.encode([7u8; 32]);
        let err = signing_key_from_pem(&raw_seed_b64).unwrap_err();
        assert!(
            err.to_string().contains("PKCS8 PEM"),
            "raw seed must be rejected as not-PEM, got: {err}"
        );
    }

    #[test]
    fn loads_from_path() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("k.key");
        std::fs::write(&path, sample_pem()).unwrap();
        let key = load_signing_key(Some(&path), DEFAULT_KEY_ENV).unwrap();
        assert_eq!(key.to_bytes(), [7u8; 32]);
    }

    #[test]
    fn missing_path_and_unset_env_errors() {
        // A never-set env var name keeps the test independent of the runner env.
        let err = load_signing_key(None, "GREENTIC_EXT_TEST_KEY_DEFINITELY_UNSET").unwrap_err();
        assert!(err.to_string().contains("not set"), "got: {err}");
    }
}
