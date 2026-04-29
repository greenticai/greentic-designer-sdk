//! Embedded WIT resources accessor.

use include_dir::{Dir, include_dir};

/// Version of the embedded WIT contract (the `@X.Y.Z` in each WIT
/// `package greentic:extension-*@X.Y.Z;` declaration). Decoupled from the
/// crate `CARGO_PKG_VERSION` because the tooling bumps faster than the WIT
/// contract — scaffolded extensions import the contract at this version.
/// Bump this constant when the vendored `wit/*.wit` files declare a new
/// `@version`.
pub const CONTRACT_VERSION: &str = "0.1.0";

static EMBEDDED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/embedded-wit/$CARGO_PKG_VERSION");

pub struct WitFile {
    pub name: &'static str,
    pub bytes: &'static [u8],
}

pub fn wit_files() -> Vec<WitFile> {
    EMBEDDED
        .files()
        .map(|f| WitFile {
            name: f
                .path()
                .file_name()
                .and_then(|s| s.to_str())
                .expect("embedded wit filename"),
            bytes: f.contents(),
        })
        .collect()
}

/// Returns the subset of WIT files needed to scaffold an extension of the given kind.
/// Always includes `extension-base.wit` and `extension-host.wit`.
///
/// `wasm-component` reuses the `design` WIT files: the scaffolded world imports
/// `greentic:extension-design/tools@0.1.0`, so `cargo component build` needs the
/// same package set as a `design` extension.
pub fn files_for_kind(kind: &str) -> Vec<WitFile> {
    let kind_file = match kind {
        "wasm-component" => "extension-design.wit".to_string(),
        other => format!("extension-{other}.wit"),
    };
    wit_files()
        .into_iter()
        .filter(|f| {
            matches!(f.name, "extension-base.wit" | "extension-host.wit") || f.name == kind_file
        })
        .collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("write to string");
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
        assert_eq!(files.len(), 7);
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

    #[test]
    fn sha256_hex_is_deterministic() {
        let a = sha256_hex(b"hello");
        let b = sha256_hex(b"hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }
}
