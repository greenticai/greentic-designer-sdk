//! Filesystem-safe filename sanitization for names sourced from free-form,
//! user-authored `describe.json` metadata (`metadata.name`).
//!
//! `metadata.name` is display text — an author can put almost anything in
//! it (e.g. `"Topic / scope guardrail"`). Several call sites across
//! `gtdx dev`/`gtdx publish` and the local filesystem registry build a
//! `<name>-<version>.gtxpack` (or similar) path directly from that raw
//! string. When the name contains a path separator, the write silently
//! targets a nonexistent nested directory and fails with an unhelpful
//! `No such file or directory (os error 2)`.
//!
//! [`sanitize_filename_component`] is the single place that turns such a
//! name into a value that is safe to use as **one** path component on
//! Linux, macOS, and Windows. It must be applied at every site that builds
//! a filename/path from `metadata.name` (or an equivalent `ext_name`
//! field) — but NOT at sites that use the same raw name for display or as
//! semantic metadata (JSON payloads, index entries, error messages), which
//! should keep showing the author's real name.

/// Replace characters that are illegal — or merely dangerous — as a single
/// path component on Linux, macOS, or Windows with `_`, then guard against
/// degenerate results by substituting a fixed placeholder. The output is
/// always non-empty, contains no path separator, and never resolves to `.`
/// or `..`.
///
/// Characters replaced:
/// - `/` and `\` — path separators on Unix/Windows respectively; the root
///   cause of the bug this function fixes.
/// - `: * ? " < > |` — additionally reserved on Windows.
/// - Any control character (including NUL), which is unsafe or outright
///   rejected by some filesystems.
///
/// Everything else (spaces, unicode, punctuation like `-`/`_`/`.`) passes
/// through unchanged, so names that are already safe are byte-for-byte
/// identical to their input — existing artifacts like
/// `PII-masking guardrail-0.1.0.gtxpack` keep their exact shape.
#[must_use]
pub fn sanitize_filename_component(raw: &str) -> String {
    let replaced: String = raw
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    if replaced.is_empty() || replaced == "." || replaced == ".." {
        "unnamed-extension".to_string()
    } else {
        replaced
    }
}

/// Build the canonical `<name>-<version>.gtxpack` filename: `name` is
/// free-form `describe.json` metadata and is sanitized via
/// [`sanitize_filename_component`]; `version` is schema-constrained semver
/// and is used verbatim.
#[must_use]
pub fn safe_pack_filename(name: &str, version: &str) -> String {
    format!("{}-{version}.gtxpack", sanitize_filename_component(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_is_replaced_so_the_name_stays_a_single_path_component() {
        let got = sanitize_filename_component("Topic / scope guardrail");
        assert!(!got.contains('/'), "must not contain '/': {got:?}");
        assert_eq!(got, "Topic _ scope guardrail");
    }

    #[test]
    fn backslash_is_replaced_so_the_name_stays_a_single_path_component() {
        let got = sanitize_filename_component(r"weird\name");
        assert!(!got.contains('\\'), "must not contain '\\': {got:?}");
        assert_eq!(got, "weird_name");
    }

    #[test]
    fn empty_name_gets_a_placeholder() {
        assert_eq!(sanitize_filename_component(""), "unnamed-extension");
    }

    #[test]
    fn dot_gets_a_placeholder() {
        assert_eq!(sanitize_filename_component("."), "unnamed-extension");
    }

    #[test]
    fn dot_dot_gets_a_placeholder() {
        assert_eq!(sanitize_filename_component(".."), "unnamed-extension");
    }

    #[test]
    fn name_that_is_only_slashes_gets_a_placeholder_free_result() {
        // "/" alone becomes "_" — not empty, not ".", not "..", so no
        // placeholder kicks in, but it must still be a safe single component.
        let got = sanitize_filename_component("/");
        assert_eq!(got, "_");
    }

    #[test]
    fn already_safe_names_pass_through_unchanged() {
        for safe in [
            "PII-masking guardrail",
            "Prompt-injection guardrail",
            "Secrets-leak guardrail",
            "Profanity guardrail",
            "demo",
        ] {
            assert_eq!(sanitize_filename_component(safe), safe);
        }
    }

    #[test]
    fn windows_reserved_characters_are_replaced() {
        let got = sanitize_filename_component(r#"a:b*c?d"e<f>g|h"#);
        for bad in [':', '*', '?', '"', '<', '>', '|'] {
            assert!(!got.contains(bad), "must not contain {bad:?}: {got:?}");
        }
    }

    #[test]
    fn safe_pack_filename_sanitizes_name_but_not_version() {
        assert_eq!(
            safe_pack_filename("Topic / scope guardrail", "0.1.0"),
            "Topic _ scope guardrail-0.1.0.gtxpack"
        );
        assert_eq!(safe_pack_filename("demo", "0.1.0"), "demo-0.1.0.gtxpack");
    }
}
