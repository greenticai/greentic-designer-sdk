//! A `.wit` file that does not parse is worse than no file: it reads as a
//! contract, reviews as a contract, and fails the first time anyone points a
//! toolchain at it. Nothing else in this repo parses `wit/` — the scaffold
//! copies the bytes and `cargo component` parses them later, in another
//! crate, at another time.
//!
//! `wit/` holds eight (now nine) top-level packages side by side with no
//! `deps/` subtree — that layout is load-bearing for `build.rs`, the
//! scaffold, and the other two file-enumeration guards in this crate, so it
//! is not something a test gets to change. `Resolve::push_dir` parses every
//! `*.wit` file under one directory as a *single* package, which is the
//! wrong shape for this directory: pointed at `wit/` it fails immediately
//! with "package identifier `greentic:extension-bundle@0.2.0` does not match
//! previous package name of `greentic:extension-base@0.2.0`". So this file
//! pushes each package individually with `Resolve::push_file`, in an order
//! that satisfies cross-package `use`/`import` — `extension-base` and
//! `extension-host` first (nothing here depends on anything else), then the
//! rest, then `runtime-side` last since it is the only file that imports
//! `extension-design`.

use std::path::{Path, PathBuf};

fn wit_root() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("wit");
    root.is_dir().then_some(root)
}

/// Sort key that puts dependency-free packages first and `runtime-side`
/// (which imports `extension-design`) last. Everything else has no
/// cross-file dependency on a sibling in `wit/`, so alphabetical order among
/// them is safe.
fn push_rank(name: &str) -> u8 {
    match name {
        "extension-base.wit" => 0,
        "extension-host.wit" => 1,
        "runtime-side.wit" => u8::MAX,
        _ => u8::MAX / 2,
    }
}

fn push_all_packages(resolve: &mut wit_parser::Resolve, root: &Path) -> anyhow::Result<()> {
    let mut names: Vec<String> = std::fs::read_dir(root)?
        .flatten()
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("wit"))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_by(|a, b| push_rank(a).cmp(&push_rank(b)).then_with(|| a.cmp(b)));
    for name in names {
        resolve
            .push_file(root.join(&name))
            .map_err(|e| anyhow::anyhow!("failed to parse wit/{name}: {e:?}"))?;
    }
    Ok(())
}

#[test]
fn the_addon_contract_parses() {
    let Some(root) = wit_root() else {
        eprintln!("workspace wit/ not present (likely packaged tarball) — skipping");
        return;
    };

    let mut resolve = wit_parser::Resolve::default();
    // Parsing the whole directory (package by package) resolves
    // `extension-addon`'s import of `extension-base/types` for real, rather
    // than checking its syntax in isolation and discovering the dangling
    // reference downstream.
    push_all_packages(&mut resolve, &root)
        .unwrap_or_else(|e| panic!("wit/ must parse as a resolvable set: {e:?}"));

    let found = resolve
        .packages
        .iter()
        .any(|(_, p)| p.name.namespace == "greentic" && p.name.name == "extension-addon");
    assert!(
        found,
        "greentic:extension-addon must be among the parsed packages"
    );
}

/// The five interfaces are the contract's shape; a rename or an accidental
/// deletion should fail here rather than in a downstream repo.
#[test]
fn the_addon_contract_declares_its_five_interfaces() {
    let Some(root) = wit_root() else {
        eprintln!("workspace wit/ not present — skipping");
        return;
    };
    let mut resolve = wit_parser::Resolve::default();
    push_all_packages(&mut resolve, &root).expect("wit/ parses");

    let pkg = resolve
        .packages
        .iter()
        .find(|(_, p)| p.name.namespace == "greentic" && p.name.name == "extension-addon")
        .map(|(_, p)| p)
        .expect("extension-addon package present");

    for want in ["types", "validation", "workload", "reconciler", "backup"] {
        assert!(
            pkg.interfaces.contains_key(want),
            "extension-addon must declare interface {want:?}; has {:?}",
            pkg.interfaces.keys().collect::<Vec<_>>()
        );
    }
}

/// `backup` is optional by design (spec D19): a world exports it only when the
/// addon can genuinely snapshot. Two worlds is how WIT expresses that, since
/// it has no optional export.
#[test]
fn backup_is_optional_via_two_worlds() {
    let Some(root) = wit_root() else {
        eprintln!("workspace wit/ not present — skipping");
        return;
    };
    let mut resolve = wit_parser::Resolve::default();
    push_all_packages(&mut resolve, &root).expect("wit/ parses");

    let pkg = resolve
        .packages
        .iter()
        .find(|(_, p)| p.name.namespace == "greentic" && p.name.name == "extension-addon")
        .map(|(_, p)| p)
        .expect("extension-addon package present");

    assert!(pkg.worlds.contains_key("addon-extension"));
    assert!(pkg.worlds.contains_key("addon-extension-with-backup"));
}
