use std::{fs, path::Path};

fn main() -> anyhow::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(manifest_dir)
        .parent()
        .and_then(Path::parent)
        .expect("crate is under workspace root");
    let src_dir = workspace_root.join("wit");
    let dst_dir = Path::new(manifest_dir).join("embedded-wit").join(version);

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
