//! Build a package: optional manifest from the directory, then a deterministic zip.

use crate::models::PackageKind;
use crate::package::manifest::Manifest;
use crate::package::validate::{self, Checked};
use crate::package::{rules, Layout};
use crate::{Error, Result};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use zip::write::FileOptions;

/// `write_manifest`: derive the allowlist from the package directory (refusing on
/// the first unsafe entry). Then write the zip and run the full check.
pub fn build(
    dev_path: &Path,
    package_id: &str,
    kind: PackageKind,
    write_manifest: bool,
) -> Result<Checked> {
    rules::validate_package_id(package_id)?;
    let layout = Layout::new(dev_path, package_id);
    if !layout.package_dir.is_dir() {
        return Err(Error::NotFound(format!(
            "package directory {}",
            layout.package_dir.display()
        )));
    }
    if write_manifest {
        let mut files = BTreeSet::new();
        validate::walk(&layout.package_dir, &layout.package_dir, &mut files)?;
        if files.is_empty() {
            return Err(Error::UnsafePackage("package directory is empty".into()));
        }
        let manifest = Manifest {
            version: 1,
            package_id: package_id.to_string(),
            kind,
            files: files.into_iter().collect(),
            client_named: vec![],
        };
        manifest.validate(package_id)?;
        manifest.write(&layout.manifest_path)?;
    }
    let manifest = Manifest::read(&layout.manifest_path)?;
    manifest.validate(package_id)?;
    write_zip(&layout, &manifest)?;
    validate::check(dev_path, package_id)
}

fn write_zip(layout: &Layout, manifest: &Manifest) -> Result<()> {
    let tmp = layout
        .delivery_dir
        .join(format!("{}.zip.tmp", manifest.package_id));
    let file = fs::File::create(&tmp).map_err(|e| Error::PathUnavailable(tmp.clone(), e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644)
        .last_modified_time(zip::DateTime::default());
    let mut files = manifest.files.clone();
    files.sort();
    let mut buf = Vec::new();
    for name in &files {
        let src = layout.package_dir.join(name);
        let mut f = fs::File::open(&src).map_err(|e| Error::PathUnavailable(src.clone(), e))?;
        buf.clear();
        f.read_to_end(&mut buf)?;
        zip.start_file(name, options)
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;
        zip.write_all(&buf)?;
    }
    zip.finish()
        .map_err(|e| Error::Io(std::io::Error::other(e)))?;
    fs::rename(&tmp, &layout.zip_path)
        .map_err(|e| Error::PathUnavailable(layout.zip_path.clone(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        let dev = dir.path().join("p");
        fs::create_dir_all(dev.join("delivery/x-v1/sub")).unwrap();
        fs::write(dev.join("delivery/x-v1/a.txt"), "a").unwrap();
        fs::write(dev.join("delivery/x-v1/sub/b.txt"), "b").unwrap();
        let first = build(&dev, "x-v1", PackageKind::Full, true).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(dev.join("delivery/x-v1/a.txt"), "a").unwrap();
        let second = build(&dev, "x-v1", PackageKind::Full, true).unwrap();
        assert_eq!(first.zip_sha256, second.zip_sha256);
        assert_eq!(first.files, vec!["a.txt", "sub/b.txt"]);
    }
}
