//! Aggregated pre-publish describe.json validation.
//!
//! Network permissions (`runtime.permissions.network`) must be `https://`,
//! with one exception: plain `http://` is accepted when the pattern host is a
//! loopback host (`127.0.0.1`, `localhost`, `[::1]`). This mirrors the
//! enforcement in `greentic-ext-runtime` (v1.2.19+), which honours plain http
//! for loopback hosts only and drops non-loopback http patterns. Keeping the
//! validator in step with the runtime means every describe the validator
//! accepts is one the runtime will actually honour — see the `http_pattern_host`
//! / `is_loopback_host` cross-reference comments below.

use greentic_extension_sdk_contract::DescribeJson;
use greentic_extension_sdk_contract::extension_id::validate_extension_id;
use semver::Version;

/// Validate describe for publish. All violations are collected before returning.
pub fn validate_for_publish(describe: &DescribeJson) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    if Version::parse(&describe.metadata.version).is_err() {
        errors.push(ValidationError::new(
            "metadata.version",
            format!("'{}' is not a valid semver", describe.metadata.version),
        ));
    }
    if let Err(e) = validate_extension_id(&describe.metadata.id) {
        errors.push(ValidationError::new("metadata.id", e.to_string()));
    }
    for (i, cap) in describe.capabilities.offered.iter().enumerate() {
        if Version::parse(&cap.version).is_err() {
            errors.push(ValidationError::new(
                format!("capabilities.offered[{i}].version"),
                format!("'{}' — not a valid semver", cap.version),
            ));
        }
    }
    for (i, url) in describe.runtime.permissions.network.iter().enumerate() {
        if url.starts_with("https://") {
            continue;
        }
        // Loopback-http exception: the runtime (greentic-ext-runtime v1.2.19)
        // honours plain `http://` for loopback hosts only — it drops
        // non-loopback http patterns with a warn and never opens cleartext to
        // public hosts. The publish validator must accept exactly the same set
        // the runtime would honour, so a valid loopback-declaring extension
        // (e.g. telco-x, `http://127.0.0.1:8787/*`) is publishable.
        if http_pattern_host(url).is_some_and(is_loopback_host) {
            continue;
        }
        errors.push(ValidationError::new(
            format!("runtime.permissions.network[{i}]"),
            format!("'{url}' — must be https:// (plain http allowed only for loopback hosts)"),
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Return the host portion of a plain-`http://` pattern, or `None` when the
/// pattern is not plain http. The leading `*.` wildcard label is stripped so
/// the remaining host can be classified; a bare wildcard host is treated as
/// non-loopback.
///
/// Bracketed IPv6 literals (e.g. `[::1]` in `http://[::1]:8787/*`) are returned
/// with their brackets intact so that [`is_loopback_host`] can strip them:
/// splitting on the first `:` would otherwise yield the bare `"["` opener and
/// misclassify `[::1]` as non-loopback.
///
/// CROSS-REFERENCE: this MUST stay semantically identical to
/// `greentic-ext-runtime`'s `http_pattern_host` in
/// `greentic-designer-extensions/crates/greentic-ext-runtime/src/loaded.rs`,
/// which the runtime uses to decide which http patterns to honour. The runtime
/// version, verbatim:
///
/// ```ignore
/// fn http_pattern_host(pattern: &str) -> Option<&str> {
///     let rest = pattern.strip_prefix("http://")?;
///     let host_and_port = rest.split('/').next().unwrap_or(rest);
///     let host_and_port = host_and_port.rsplit('@').next().unwrap_or(host_and_port);
///     let host = if let Some(bracket_end) = host_and_port.find(']') {
///         &host_and_port[..=bracket_end]
///     } else {
///         host_and_port.split(':').next().unwrap_or(host_and_port)
///     };
///     Some(host.trim_start_matches("*."))
/// }
/// ```
fn http_pattern_host(pattern: &str) -> Option<&str> {
    let rest = pattern.strip_prefix("http://")?;
    let host_and_port = rest.split('/').next().unwrap_or(rest);
    // Strip the userinfo (`user@host`) if present.
    let host_and_port = host_and_port.rsplit('@').next().unwrap_or(host_and_port);
    // Bracketed IPv6 literal: `[::1]` or `[::1]:8787`. Return the bracketed
    // token (including the `]`) so is_loopback_host can strip the brackets and
    // compare against `::1`.
    let host = if let Some(bracket_end) = host_and_port.find(']') {
        &host_and_port[..=bracket_end]
    } else {
        // Plain hostname or IPv4: split on first `:` to drop optional port.
        host_and_port.split(':').next().unwrap_or(host_and_port)
    };
    Some(host.trim_start_matches("*."))
}

/// Loopback hosts for which plain http is acceptable: `localhost`, `127.0.0.1`,
/// and the IPv6 loopback `::1`. The match is EXACT (not a prefix), so an
/// adversarial public host like `127.0.0.1.evil.com` is correctly rejected.
///
/// CROSS-REFERENCE: this MUST stay semantically identical to
/// `greentic-ext-runtime`'s `is_loopback_host` in
/// `greentic-designer-extensions/crates/greentic-ext-runtime/src/loaded.rs`.
/// The runtime version, verbatim:
///
/// ```ignore
/// fn is_loopback_host(host: &str) -> bool {
///     let host = host.trim_start_matches('[').trim_end_matches(']');
///     host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
/// }
/// ```
fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Human-readable formatter for a collection of errors.
pub fn format_errors(errors: &[ValidationError]) -> String {
    use std::fmt::Write as _;
    let mut out = format!(
        "\u{2717} describe.json validation failed ({} errors):\n",
        errors.len()
    );
    for e in errors {
        let _ = writeln!(&mut out, "  \u{2022} {}: {}", e.field, e.message);
    }
    out.push_str("\nFix these and re-run: gtdx publish\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_extension_sdk_contract::{
        DescribeJson, ExtensionKind,
        describe::{Author, Capabilities, Engine, Metadata, Permissions, Runtime},
    };

    fn sample_describe() -> DescribeJson {
        DescribeJson {
            secret_requirements: Vec::new(),
            schema_ref: None,
            api_version: "greentic.ai/v2".into(),
            kind: ExtensionKind::Design,
            compat: greentic_extension_sdk_contract::Compat {
                min_designer_version: ">=1.0.0".parse().unwrap(),
                min_runner_version: "^0.12.0".parse().unwrap(),
                contract_version: "1.2.0".parse().unwrap(),
            },
            metadata: Metadata {
                id: "com.example.demo".into(),
                name: "demo".into(),
                version: "0.1.0".into(),
                summary: greentic_extension_sdk_contract::LocalizedString::plain("s"),
                description: None,
                author: Author {
                    name: "a".into(),
                    email: None,
                    public_key: None,
                },
                license: "MIT".into(),
                homepage: None,
                repository: None,
                keywords: vec![],
                icon: None,
                screenshots: vec![],
            },
            engine: Some(Engine {
                greentic_designer: "^0.1".into(),
                ext_runtime: "^0.1".into(),
            }),
            capabilities: Capabilities {
                offered: vec![],
                required: vec![],
            },
            runtime: Runtime {
                world: None,
                memory_limit_mb: 64,
                permissions: Permissions::default(),
                components: std::collections::BTreeMap::new(),
            },
            execution: None,
            contributions: greentic_extension_sdk_contract::describe::Contributions::default(),
            localization: None,
            signature: None,
            manifest_sha256: None,
            required_secrets: vec![],
        }
    }

    #[test]
    fn valid_describe_passes() {
        assert!(validate_for_publish(&sample_describe()).is_ok());
    }

    #[test]
    fn bad_version_reports_error() {
        let mut d = sample_describe();
        d.metadata.version = "0.1".into();
        let errs = validate_for_publish(&d).unwrap_err();
        assert!(errs.iter().any(|e| e.field == "metadata.version"));
    }

    #[test]
    fn bad_id_reports_error() {
        let mut d = sample_describe();
        d.metadata.id = "NoDots".into();
        let errs = validate_for_publish(&d).unwrap_err();
        assert!(errs.iter().any(|e| e.field == "metadata.id"));
    }

    #[test]
    fn digits_inside_a_word_are_publishable() {
        let mut d = sample_describe();
        d.metadata.id = "greentic.aigent3-designer".into();
        let errs = validate_for_publish(&d).err().unwrap_or_default();
        assert!(
            !errs.iter().any(|e| e.field == "metadata.id"),
            "unexpected id error: {errs:?}"
        );
    }

    /// Publishing an id whose WIT package name is invalid ships an extension
    /// nobody can rebuild from source.
    #[test]
    fn a_digit_led_word_is_not_publishable() {
        let mut d = sample_describe();
        d.metadata.id = "greentic.provider-3aigent".into();
        let errs = validate_for_publish(&d).unwrap_err();
        assert!(errs.iter().any(|e| e.field == "metadata.id"), "{errs:?}");
    }

    #[test]
    fn non_greentic_namespace_is_publishable() {
        let mut d = sample_describe();
        d.metadata.id = "com.acme.my-ext".into();
        let errs = validate_for_publish(&d).err().unwrap_or_default();
        assert!(
            !errs.iter().any(|e| e.field == "metadata.id"),
            "unexpected id error: {errs:?}"
        );
    }

    #[test]
    fn bad_id_message_names_the_offending_part() {
        let mut d = sample_describe();
        d.metadata.id = "greentic.Telco".into();
        let errs = validate_for_publish(&d).unwrap_err();
        let msg = &errs
            .iter()
            .find(|e| e.field == "metadata.id")
            .expect("id error")
            .message;
        assert!(msg.contains("Telco"), "{msg}");
        assert!(msg.contains("lowercase"), "{msg}");
    }

    #[test]
    fn http_permission_is_rejected() {
        let mut d = sample_describe();
        d.runtime.permissions.network = vec!["http://insecure.com".into()];
        let errs = validate_for_publish(&d).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.field == "runtime.permissions.network[0]")
        );
    }

    /// Loopback-http rule (mirrors greentic-ext-runtime v1.2.19): a declared
    /// `http://127.0.0.1...` pattern is valid for publish because the runtime
    /// honours plain http for loopback hosts.
    #[test]
    fn http_loopback_127_is_allowed() {
        let mut d = sample_describe();
        d.runtime.permissions.network = vec!["http://127.0.0.1:8787/*".into()];
        assert!(
            validate_for_publish(&d).is_ok(),
            "loopback 127.0.0.1 http pattern must pass publish validation"
        );
    }

    /// `http://localhost...` is the second loopback spelling and must pass.
    #[test]
    fn http_loopback_localhost_is_allowed() {
        let mut d = sample_describe();
        d.runtime.permissions.network = vec!["http://localhost:3000/*".into()];
        assert!(
            validate_for_publish(&d).is_ok(),
            "loopback localhost http pattern must pass publish validation"
        );
    }

    /// Bracketed IPv6 loopback `http://[::1]...` must pass; the host extraction
    /// must strip brackets exactly like the ext-runtime implementation.
    #[test]
    fn http_loopback_ipv6_is_allowed() {
        let mut d = sample_describe();
        d.runtime.permissions.network = vec!["http://[::1]:8787/*".into()];
        assert!(
            validate_for_publish(&d).is_ok(),
            "loopback [::1] http pattern must pass publish validation"
        );
    }

    /// A non-loopback http host must still be rejected — no plain-http
    /// downgrade for public hosts.
    #[test]
    fn http_non_loopback_is_rejected() {
        let mut d = sample_describe();
        d.runtime.permissions.network = vec!["http://evil.com/*".into()];
        let errs = validate_for_publish(&d).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.field == "runtime.permissions.network[0]"),
            "non-loopback http must be rejected"
        );
    }

    /// Adversarial: a host that merely starts with `127.0.0.1` but is actually
    /// a public domain (`127.0.0.1.evil.com`) must be rejected. The host match
    /// is exact, not a prefix.
    #[test]
    fn http_loopback_prefix_smuggle_is_rejected() {
        let mut d = sample_describe();
        d.runtime.permissions.network = vec!["http://127.0.0.1.evil.com/*".into()];
        let errs = validate_for_publish(&d).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.field == "runtime.permissions.network[0]"),
            "loopback-prefixed public host must be rejected (exact host match only)"
        );
    }

    #[test]
    fn errors_aggregate_all_violations() {
        let mut d = sample_describe();
        d.metadata.version = "0.1".into();
        d.metadata.id = "BAD".into();
        d.runtime.permissions.network = vec!["http://insecure.com".into()];
        let errs = validate_for_publish(&d).unwrap_err();
        assert_eq!(errs.len(), 3);
    }

    #[test]
    fn format_errors_lists_all_fields() {
        let errs = vec![
            ValidationError::new("metadata.version", "bad"),
            ValidationError::new("metadata.id", "bad"),
        ];
        let s = format_errors(&errs);
        assert!(s.contains("2 errors"));
        assert!(s.contains("metadata.version"));
        assert!(s.contains("metadata.id"));
    }
}
