//! Shared opt-in for non-HTTPS Greentic Store URLs.
//!
//! The `greentic-extension-sdk-registry` Store backend refuses cleartext HTTP
//! to anything other than loopback to keep bearer tokens and signed describes
//! off the wire. Some deployments (e.g. the migration window before a Store
//! gets fronted by TLS, private-VPC CI) need a documented escape hatch — the
//! [`insecure_registry_opt_in`] env-var lookup centralises that decision so
//! every CLI entry point (`publish`, `install`, `search`) honours the same
//! truthy-value parsing.

/// Env var name read by [`insecure_registry_opt_in`]. Pulled out as a const
/// so docs / error messages / tests refer to one canonical spelling.
pub const INSECURE_REGISTRY_ENV: &str = "GTDX_ALLOW_INSECURE_REGISTRY";

/// Returns `true` when [`INSECURE_REGISTRY_ENV`] is set to any value other
/// than empty / `0` / `false` / `no` (case-insensitive). The flag is the only
/// documented escape hatch for talking to a Greentic Store that has not yet
/// been fronted by TLS.
pub fn insecure_registry_opt_in() -> bool {
    parse_truthy(std::env::var(INSECURE_REGISTRY_ENV).ok().as_deref())
}

/// Pure-function half of [`insecure_registry_opt_in`] kept testable without
/// poking at the process-wide environment (which would force every test to
/// either run sequentially or wrap unsafe `set_var` calls — neither plays
/// nicely with `#![forbid(unsafe_code)]`).
fn parse_truthy(value: Option<&str>) -> bool {
    match value {
        Some(raw) => {
            let normalized = raw.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "" | "0" | "false" | "no")
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_truthy;

    #[test]
    fn unset_is_default_secure() {
        assert!(!parse_truthy(None));
    }

    #[test]
    fn truthy_values_opt_in() {
        for v in ["1", "true", "TRUE", "yes", "on", "  1  "] {
            assert!(parse_truthy(Some(v)), "value {v:?} should opt in");
        }
    }

    #[test]
    fn falsy_values_stay_secure() {
        for v in ["", "0", "false", "FALSE", "no", "  0  "] {
            assert!(!parse_truthy(Some(v)), "value {v:?} should stay secure");
        }
    }
}
