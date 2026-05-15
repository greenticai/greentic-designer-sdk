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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_and_unpack_preserves_nested_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("packed.gtxpack");
        let out = tmp.path().join("out");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("root.txt"), "root").unwrap();
        std::fs::write(src.join("nested/file.txt"), "nested").unwrap();

        pack_directory(&src, &dest).unwrap();
        unpack_to_dir(&dest, &out).unwrap();

        assert_eq!(
            std::fs::read_to_string(out.join("root.txt")).unwrap(),
            "root"
        );
        assert_eq!(
            std::fs::read_to_string(out.join("nested/file.txt")).unwrap(),
            "nested"
        );
    }

    #[test]
    fn unpack_creates_destination_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let pack = tmp.path().join("single.gtxpack");
        let out = tmp.path().join("missing/out");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("file.txt"), "hello").unwrap();

        pack_directory(&src, &pack).unwrap();
        unpack_to_dir(&pack, &out).unwrap();

        assert_eq!(
            std::fs::read_to_string(out.join("file.txt")).unwrap(),
            "hello"
        );
    }
}
