//! Cross-checks that prevent silent drift between
//!   - `CARGO_PKG_VERSION` (workspace + crate version)
//!   - `embedded-wit/$CARGO_PKG_VERSION/` (the WIT files baked into the binary)
//!   - the WIT package `@version` declarations under `wit/`
//!   - the `CONTRACT_VERSION` constant scaffolded into new projects
//!
//! Audit P1: contract version drift across SDK (0.1.0 / 0.2.0 / 0.4.0 /
//! 0.4.4 in different files). The fix is a guard, not a one-time cleanup —
//! these tests fail loudly the next time anyone bumps one half without the
//! other.

#[test]
fn embedded_wit_directory_matches_cargo_pkg_version() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let pkg = env!("CARGO_PKG_VERSION");
    let candidate = std::path::Path::new(manifest_dir)
        .join("embedded-wit")
        .join(pkg);
    assert!(
        candidate.exists(),
        "embedded-wit/{pkg} must exist (CARGO_MANIFEST_DIR={manifest_dir}). build.rs auto-creates it from workspace wit/; if you renamed the dir, also bump the version it tracks.",
    );
}

#[test]
fn no_legacy_embedded_wit_directories() {
    // Legacy `embedded-wit/` directories from prior workspace versions
    // should be deleted when the version bumps. Keeping them around invites
    // a developer to copy-paste the wrong `embedded-wit/X.Y.Z/` and silently
    // ship stale WIT files when downgrading the workspace version.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let pkg = env!("CARGO_PKG_VERSION");
    let embedded_root = std::path::Path::new(manifest_dir).join("embedded-wit");
    let entries: Vec<String> = std::fs::read_dir(&embedded_root)
        .expect("read embedded-wit/ dir")
        .filter_map(std::result::Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries,
        vec![pkg.to_string()],
        "embedded-wit/ must contain exactly one subdirectory matching CARGO_PKG_VERSION ({pkg}); found {entries:?}. Delete legacy version-named subdirs.",
    );
}

#[test]
fn wit_files_declare_consistent_package_version() {
    // Each wit/extension-*.wit file must declare its pinned `@X.Y.Z` package
    // version (see the per-file map below). This is the surface scaffolded
    // extensions import against; if a file drifts from its pin, scaffolded
    // `wit/deps/greentic/<pkg>/world.wit` files would import an incompatible
    // contract.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let wit_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .join("wit");
    if !wit_root.exists() {
        // When tests run against a packaged tarball there is no workspace
        // wit/ next to the crate; skip rather than fail.
        eprintln!("workspace wit/ not present (likely packaged tarball) — skipping");
        return;
    }
    // Per-file expected @version. CONTRACT_VERSION is the contract generation
    // (the shared base all worlds import); extension-design carries an internal
    // increment for its `roles` interface, so it is one minor ahead.
    let expected: &[(&str, &str)] = &[
        ("extension-base.wit", "0.2.0"),
        ("extension-design.wit", "0.3.0"),
        ("extension-bundle.wit", "0.2.0"),
        ("extension-deploy.wit", "0.2.0"),
        ("extension-provider.wit", "0.2.0"),
        ("extension-host.wit", "0.1.0"),
        ("extension-dw-composer.wit", "0.2.0"),
        ("runtime-side.wit", "0.2.0"),
        ("extension-addon.wit", "0.1.0"),
    ];
    for (name, want) in expected {
        let path = wit_root.join(name);
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read wit/{name}: {e}"));
        let first_line = text.lines().next().unwrap_or_default();
        let at = first_line
            .find('@')
            .unwrap_or_else(|| panic!("wit/{name} declares no @version: {first_line:?}"));
        let after = &first_line[at + 1..];
        let semi = after.find(';').unwrap_or(after.len());
        let got = after[..semi].trim();
        assert_eq!(got, *want, "wit/{name} declares @{got}, expected @{want}");
    }
    let constant_version = greentic_extension_sdk_cli_for_tests::contract_version();
    assert_eq!(
        constant_version, "0.2.0",
        "CONTRACT_VERSION ({constant_version}) must equal the base contract generation 0.2.0",
    );
    // Every wit file on disk must be covered by the expected map, so a newly
    // added contract file can't silently skip version assertion.
    let on_disk: std::collections::BTreeSet<String> = std::fs::read_dir(&wit_root)
        .expect("read wit/")
        .flatten()
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("wit"))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let covered: std::collections::BTreeSet<String> =
        expected.iter().map(|(n, _)| (*n).to_string()).collect();
    assert_eq!(
        on_disk,
        covered,
        "wit/ files not covered by expected-version map: {:?}",
        on_disk.difference(&covered).collect::<Vec<_>>()
    );
}

/// Tiny helper crate-local module to expose the `CONTRACT_VERSION` constant
/// to the integration test (the const is private to the binary crate).
mod greentic_extension_sdk_cli_for_tests {
    /// Re-reads `CONTRACT_VERSION` from the source file at compile time.
    /// `include_str!` is the cheapest way to get a const value out of a
    /// `pub(crate)` constant in a binary crate without restructuring it.
    pub fn contract_version() -> &'static str {
        const SRC: &str = include_str!("../src/scaffold/embedded.rs");
        // Find: `pub const CONTRACT_VERSION: &str = "X.Y.Z";`
        let needle = "pub const CONTRACT_VERSION: &str = \"";
        let start = SRC.find(needle).expect("CONTRACT_VERSION decl present") + needle.len();
        let end = SRC[start..].find('"').expect("closing quote") + start;
        Box::leak(SRC[start..end].to_string().into_boxed_str())
    }
}
