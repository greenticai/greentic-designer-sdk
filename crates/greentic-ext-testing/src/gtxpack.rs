use std::io::Write;
use std::path::Path;

use anyhow::Result;
use zip::write::SimpleFileOptions;

pub fn pack_directory(src: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    walk_and_add(src, src, &mut zip, opts)?;
    zip.finish()?;
    Ok(())
}

fn walk_and_add<W: Write + std::io::Seek>(
    root: &Path,
    current: &Path,
    zip: &mut zip::ZipWriter<W>,
    opts: SimpleFileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(root)?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if entry.file_type()?.is_dir() {
            walk_and_add(root, &path, zip, opts)?;
        } else {
            zip.start_file(rel_str, opts)?;
            let mut f = std::fs::File::open(&path)?;
            std::io::copy(&mut f, zip)?;
        }
    }
    Ok(())
}

pub fn unpack_to_dir(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let file = std::fs::File::open(src)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let outpath = dest.join(entry.mangled_name());
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)?;
            continue;
        }
        if let Some(parent) = outpath.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&outpath)?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}
