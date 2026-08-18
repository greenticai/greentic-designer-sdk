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

/// Canonical placeholder for each vendored WIT package's version.
///
/// Kept next to the assertion that enforces it so the two cannot drift.
const PACKAGE_PLACEHOLDER: &[(&str, &str)] = &[
    ("extension-base", "{{v_base}}"),
    ("extension-host", "{{v_host}}"),
    ("extension-design", "{{v_design}}"),
    ("extension-bundle", "{{v_bundle}}"),
    ("extension-deploy", "{{v_deploy}}"),
    ("extension-provider", "{{v_provider}}"),
];

/// Read `<package>.wit`'s declared `@version` from the copy embedded in the
/// binary — the same source `gtdx new` fills its placeholders from.
fn embedded_package_version(package: &str) -> Option<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("embedded-wit")
        .join(env!("CARGO_PKG_VERSION"));
    let text = std::fs::read_to_string(dir.join(format!("{package}.wit"))).ok()?;
    let first = text.lines().next()?;
    let at = first.find('@')?;
    let after = &first[at + 1..];
    let end = after.find(';').unwrap_or(after.len());
    Some(after[..end].trim().to_string())
}

fn templates_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")
}

fn world_wit_templates() -> Vec<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.file_name().and_then(|s| s.to_str()) == Some("world.wit.tmpl") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&templates_root(), &mut out);
    out.sort();
    assert!(!out.is_empty(), "no world.wit.tmpl files found");
    out
}

/// Every `greentic:<pkg>/<iface>@<ver>` in a scaffold world must carry the
/// placeholder for *that* package — never a literal, and never another
/// package's placeholder.
///
/// This is the guard that was missing. The suite already asserted each WIT
/// file's own `@version`, which made the divergence (`extension-host@0.1.0`,
/// `extension-design@0.3.0`, everything else `@0.2.0`) look accounted for —
/// while the templates stamped a single `{{contract_version}}` onto all of
/// them. Scaffolds therefore imported packages that do not exist and no test
/// noticed, because none of them ran `cargo component build`.
#[test]
fn scaffold_worlds_reference_each_package_by_its_own_placeholder() {
    let expected: std::collections::BTreeMap<&str, &str> =
        PACKAGE_PLACEHOLDER.iter().copied().collect();
    let mut problems = Vec::new();

    for path in world_wit_templates() {
        let text = std::fs::read_to_string(&path).expect("read world.wit.tmpl");
        let rel = path
            .strip_prefix(templates_root())
            .unwrap_or(&path)
            .display()
            .to_string();

        for line in text.lines() {
            // Match `greentic:<pkg>/<iface>@<token>` up to the statement end.
            let Some(at) = line.find("greentic:") else {
                continue;
            };
            let rest = &line[at + "greentic:".len()..];
            let Some(slash) = rest.find('/') else {
                continue;
            };
            let pkg = &rest[..slash];
            let Some(version_at) = rest.find('@') else {
                problems.push(format!(
                    "{rel}: `greentic:{pkg}` reference carries no @version"
                ));
                continue;
            };
            let token: String = rest[version_at + 1..]
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != ';')
                .collect();

            let Some(want) = expected.get(pkg) else {
                continue; // not a vendored greentic package (e.g. wasix:mcp)
            };
            if token != *want {
                problems.push(format!(
                    "{rel}: greentic:{pkg} is pinned to `{token}`, expected `{want}`"
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "scaffold worlds must use the per-package version placeholder:\n  {}",
        problems.join("\n  ")
    );
}

/// The placeholders above must actually resolve, and resolve to the version
/// the corresponding `wit/*.wit` file declares.
#[test]
fn every_package_placeholder_resolves_to_the_vendored_wit_version() {
    let wit_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .join("wit");
    if !wit_root.exists() {
        eprintln!("workspace wit/ not present (packaged tarball) — skipping");
        return;
    }

    for (pkg, placeholder) in PACKAGE_PLACEHOLDER {
        let key = placeholder
            .trim_start_matches("{{")
            .trim_end_matches("}}")
            .to_string();
        let declared = {
            let text = std::fs::read_to_string(wit_root.join(format!("{pkg}.wit")))
                .unwrap_or_else(|e| panic!("read wit/{pkg}.wit: {e}"));
            let first = text.lines().next().unwrap_or_default().to_string();
            let at = first
                .find('@')
                .unwrap_or_else(|| panic!("wit/{pkg}.wit declares no @version"));
            let after = &first[at + 1..];
            let end = after.find(';').unwrap_or(after.len());
            after[..end].trim().to_string()
        };
        let resolved = embedded_package_version(pkg)
            .unwrap_or_else(|| panic!("no embedded version exposed for `{pkg}` (key `{key}`)"));
        assert_eq!(
            resolved, declared,
            "placeholder `{placeholder}` resolves to {resolved}, but wit/{pkg}.wit declares {declared}"
        );
    }
}
