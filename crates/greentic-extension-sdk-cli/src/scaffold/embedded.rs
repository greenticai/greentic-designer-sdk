//! Embedded WIT resources accessor.

use include_dir::{Dir, include_dir};

/// Version of the embedded WIT contract *generation* — the shared base
/// version (`extension-base@X.Y.Z`) that every world imports. Decoupled from
/// the crate `CARGO_PKG_VERSION` because the tooling bumps faster than the WIT
/// contract — scaffolded extensions import the contract at this generation.
///
/// This is NOT a uniform per-file version: within a generation, individual
/// worlds may carry an internal increment. As of generation `0.2.0`,
/// `extension-design` is one minor ahead (`@0.3.0`) for its `roles` interface,
/// while `extension-host` is still `@0.1.0`. The per-file `@version` values are
/// asserted explicitly in `tests/contract_version_consistency.rs`.
/// Bump this constant when the shared base contract advances a generation.
pub const CONTRACT_VERSION: &str = "0.2.0";

static EMBEDDED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/embedded-wit/$CARGO_PKG_VERSION");

pub struct WitFile {
    pub name: &'static str,
    pub bytes: &'static [u8],
}

pub fn wit_files() -> Vec<WitFile> {
    // Skip any entry without a valid UTF-8 file name rather than panicking; the
    // files are compile-time-embedded so this never fires, but the no-panic
    // guardrail forbids the construct (audit cycle-2 P3).
    EMBEDDED
        .files()
        .filter_map(|f| {
            f.path()
                .file_name()
                .and_then(|s| s.to_str())
                .map(|name| WitFile {
                    name,
                    bytes: f.contents(),
                })
        })
        .collect()
}

/// Returns the subset of WIT files needed to scaffold an extension of the given kind.
/// Always includes `extension-base.wit` and `extension-host.wit` for greentic
/// extension kinds.
///
/// `wasm-component` and `llm` reuse the `design` WIT files: their scaffolded
/// worlds import `greentic:extension-design/tools@0.1.0`, so `cargo component
/// build` needs the same package set as a `design` extension.
///
/// `mcp` is the exception: it is a `wasix:mcp/router` component, not a greentic
/// design extension. It imports NO greentic WIT package — its world exports the
/// `wasix:mcp/router` interface — so it pulls in none of these embedded files.
/// The `wasix-mcp` WIT dep ships as a `templates/mcp/wit/deps/wasix-mcp/`
/// template file instead.
pub fn files_for_kind(kind: &str) -> Vec<WitFile> {
    if kind == "mcp" {
        return Vec::new();
    }
    let kind_file = match kind {
        "wasm-component" | "llm" => "extension-design.wit".to_string(),
        other => format!("extension-{other}.wit"),
    };
    wit_files()
        .into_iter()
        .filter(|f| {
            matches!(f.name, "extension-base.wit" | "extension-host.wit") || f.name == kind_file
        })
        .collect()
}

/// Extract the `@X.Y.Z` package version declared on the first
/// `package greentic:<name>@X.Y.Z;` line of an embedded WIT file's bytes.
///
/// Each embedded package (`extension-base`, `extension-host`,
/// `extension-design`, ...) is versioned independently — see
/// [`CONTRACT_VERSION`]'s doc comment — so callers that render a `world.wit`
/// importing/exporting from more than one package must read the real
/// per-file version here rather than assuming they all match
/// `CONTRACT_VERSION`. `tests/contract_version_consistency.rs` pins the
/// expected value per file; this function is the runtime counterpart used by
/// codegen that needs the same numbers.
#[must_use]
pub fn package_version(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let first_line = text.lines().next()?;
    let at = first_line.find('@')?;
    let after = &first_line[at + 1..];
    let semi = after.find(';').unwrap_or(after.len());
    Some(after[..semi].trim().to_string())
}

/// The `@version` declared by the embedded `greentic:extension-<suffix>`
/// package — e.g. `package_version_for("host")` is `"0.1.0"`.
///
/// Every renderer of a `world.wit` MUST source its versions here rather than
/// from [`CONTRACT_VERSION`]. The packages are versioned independently within
/// a generation (`extension-host` is `@0.1.0`, `extension-design` is `@0.3.0`,
/// the rest are `@0.2.0`), so a world rendered with one uniform version asks
/// for a package that does not exist and `cargo component build` fails with
/// `package 'greentic:extension-host@0.2.0' not found`.
pub fn package_version_for(suffix: &str) -> Option<String> {
    let file = format!("extension-{suffix}.wit");
    wit_files()
        .into_iter()
        .find(|f| f.name == file)
        .and_then(|f| package_version(f.bytes))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        // from_digit only returns None for radix>36, so these never fall back.
        out.push(char::from_digit(u32::from(b >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(b & 0xf), 16).unwrap_or('0'));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wit_files_returns_all_embedded_packages() {
        let files = wit_files();
        assert!(files.iter().any(|f| f.name == "extension-base.wit"));
        assert!(files.iter().any(|f| f.name == "extension-host.wit"));
        assert!(files.iter().any(|f| f.name == "extension-design.wit"));
        assert!(files.iter().any(|f| f.name == "extension-bundle.wit"));
        assert!(files.iter().any(|f| f.name == "extension-deploy.wit"));
        assert!(files.iter().any(|f| f.name == "extension-provider.wit"));
        assert!(files.iter().any(|f| f.name == "extension-dw-composer.wit"));
        assert!(files.iter().any(|f| f.name == "runtime-side.wit"));
        assert_eq!(files.len(), 8);
    }

    #[test]
    fn files_for_kind_design_includes_base_host_and_design() {
        let files = files_for_kind("design");
        let names: Vec<_> = files.iter().map(|f| f.name).collect();
        assert!(names.contains(&"extension-base.wit"));
        assert!(names.contains(&"extension-host.wit"));
        assert!(names.contains(&"extension-design.wit"));
        assert!(!names.contains(&"extension-bundle.wit"));
    }

    #[test]
    fn files_for_kind_bundle_includes_bundle_not_design() {
        let files = files_for_kind("bundle");
        let names: Vec<_> = files.iter().map(|f| f.name).collect();
        assert!(names.contains(&"extension-bundle.wit"));
        assert!(!names.contains(&"extension-design.wit"));
    }

    #[test]
    fn files_for_kind_provider_includes_provider_not_design() {
        let files = files_for_kind("provider");
        let names: Vec<_> = files.iter().map(|f| f.name).collect();
        assert!(names.contains(&"extension-base.wit"));
        assert!(names.contains(&"extension-host.wit"));
        assert!(names.contains(&"extension-provider.wit"));
        assert!(!names.contains(&"extension-design.wit"));
        assert!(!names.contains(&"extension-bundle.wit"));
        assert!(!names.contains(&"extension-deploy.wit"));
    }

    /// E.4.b: `llm` is a design-extension subtype — its WIT set must mirror
    /// `design` (no separate `extension-llm.wit`), so `cargo component build`
    /// can resolve the scaffolded world.
    #[test]
    fn files_for_kind_llm_uses_design_wit() {
        let files = files_for_kind("llm");
        let names: Vec<_> = files.iter().map(|f| f.name).collect();
        assert!(names.contains(&"extension-base.wit"));
        assert!(names.contains(&"extension-host.wit"));
        assert!(names.contains(&"extension-design.wit"));
        assert!(!names.contains(&"extension-bundle.wit"));
        assert!(!names.contains(&"extension-deploy.wit"));
        assert!(!names.contains(&"extension-provider.wit"));
    }

    /// `mcp` is a `wasix:mcp/router` component, not a greentic design
    /// extension. It imports NO greentic WIT package, so `files_for_kind`
    /// returns no embedded greentic WIT files — the `wasix-mcp` WIT dep ships
    /// as a `templates/mcp` file instead.
    #[test]
    fn files_for_kind_mcp_pulls_no_greentic_wit() {
        let files = files_for_kind("mcp");
        assert!(
            files.is_empty(),
            "mcp (wasix:mcp/router) must not bundle greentic WIT deps, got: {:?}",
            files.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn package_version_reads_the_declared_at_version() {
        assert_eq!(
            package_version(b"package greentic:extension-host@0.1.0;\n"),
            Some("0.1.0".to_string())
        );
        assert_eq!(
            package_version(b"package greentic:extension-design@0.3.0;\n\ninterface tools {}"),
            Some("0.3.0".to_string())
        );
    }

    #[test]
    fn package_version_matches_the_pinned_embedded_files() {
        // Cross-check against the same pins `tests/contract_version_consistency.rs`
        // asserts, so a template that reads `package_version` at runtime gets the
        // exact numbers other tests already guard.
        let expected: &[(&str, &str)] = &[
            ("extension-base.wit", "0.2.0"),
            ("extension-host.wit", "0.1.0"),
            ("extension-design.wit", "0.3.0"),
        ];
        for (name, want) in expected {
            let file = wit_files()
                .into_iter()
                .find(|f| f.name == *name)
                .unwrap_or_else(|| panic!("embedded {name} missing"));
            assert_eq!(package_version(file.bytes).as_deref(), Some(*want));
        }
    }

    #[test]
    fn sha256_hex_is_deterministic() {
        let a = sha256_hex(b"hello");
        let b = sha256_hex(b"hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }
}
