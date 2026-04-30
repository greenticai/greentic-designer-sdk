use std::{fs, path::Path};

fn main() -> anyhow::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dst_dir = Path::new(manifest_dir).join("embedded-wit").join(version);

    // Resolve workspace `wit/` if we're building from the workspace tree (dev,
    // CI). When verifying a packaged tarball (cargo publish, crates.io
    // install), the workspace root is not present relative to the unpacked
    // crate, so fall back to the already-populated `embedded-wit/<version>/`
    // directory that ships with the tarball.
    let src_dir = Path::new(manifest_dir)
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("wit"))
        .filter(|p| p.exists());

    let Some(src_dir) = src_dir else {
        if dst_dir.exists() {
            println!("cargo:warning=using pre-embedded WIT files at version {version}");
            return Ok(());
        }
        return Err(anyhow::anyhow!(
            "neither workspace wit/ nor embedded-wit/{version}/ found; cannot embed WIT spec"
        ));
    };

    if dst_dir.exists() {
        fs::remove_dir_all(&dst_dir)?;
    }
    fs::create_dir_all(&dst_dir)?;

    let mut count = 0usize;
    for entry in fs::read_dir(&src_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("wit") {
            let name = path.file_name().expect("wit file has name");
            fs::copy(&path, dst_dir.join(name))?;
            println!("cargo:rerun-if-changed={}", path.display());
            count += 1;
        }
    }

    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!("cargo:warning=embedded {count} WIT files at version {version}");
    Ok(())
}
