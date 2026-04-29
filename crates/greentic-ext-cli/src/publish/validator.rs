//! Aggregated pre-publish describe.json validation.

use greentic_ext_contract::DescribeJson;
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
    if !is_valid_id(&describe.metadata.id) {
        errors.push(ValidationError::new(
            "metadata.id",
            format!(
                "'{}' — must match reverse-DNS regex ^[a-z][a-z0-9-]*(\\.[a-z][a-z0-9-]*)+$",
                describe.metadata.id
            ),
        ));
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
        if !url.starts_with("https://") {
            errors.push(ValidationError::new(
                format!("runtime.permissions.network[{i}]"),
                format!("'{url}' — must be https://"),
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn is_valid_id(id: &str) -> bool {
    // Regex: ^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$
    let mut parts = id.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    if !part_is_valid(first) {
        return false;
    }
    let mut has_more = false;
    for p in parts {
        has_more = true;
        if !part_is_valid(p) {
            return false;
        }
    }
    has_more
}

fn part_is_valid(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
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
    use greentic_ext_contract::{
        DescribeJson, ExtensionKind,
        describe::{Author, Capabilities, Engine, Metadata, Permissions, Runtime},
    };

    fn sample_describe() -> DescribeJson {
        DescribeJson {
            schema_ref: None,
            api_version: "greentic.ai/v1".into(),
            kind: ExtensionKind::Design,
            metadata: Metadata {
                id: "com.example.demo".into(),
                name: "demo".into(),
                version: "0.1.0".into(),
                summary: "s".into(),
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
            engine: Engine {
                greentic_designer: "^0.1".into(),
                ext_runtime: "^0.1".into(),
            },
            capabilities: Capabilities {
                offered: vec![],
                required: vec![],
            },
            runtime: Runtime {
                component: "extension.wasm".into(),
                memory_limit_mb: 64,
                permissions: Permissions::default(),
                gtpack: None,
            },
            execution: None,
            contributions: serde_json::json!({}),
            signature: None,
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
    fn http_permission_is_rejected() {
        let mut d = sample_describe();
        d.runtime.permissions.network = vec!["http://insecure.com".into()];
        let errs = validate_for_publish(&d).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.field == "runtime.permissions.network[0]")
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
