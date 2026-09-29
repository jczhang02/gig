//! The package check: filesystem layout, manifest, directory contents, zip contents.

use crate::models::PackageKind;
use crate::package::manifest::Manifest;
use crate::package::{rules, Layout};
use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub layout: Layout,
    pub kind: PackageKind,
    pub files: Vec<String>,
    pub zip_sha256: String,
    pub zip_size: u64,
    pub warnings: Vec<String>,
}

/// Run every rule. Nothing is written.
pub fn check(dev_path: &Path, package_id: &str) -> Result<Checked> {
    rules::validate_package_id(package_id)?;
    let layout = Layout::new(dev_path, package_id);
    let dev_canon = canon_dir(dev_path, "project directory")?;
    let delivery_canon = canon_dir(&layout.delivery_dir, "delivery directory")?;
    inside(
        &delivery_canon,
        &dev_canon,
        &layout.delivery_dir,
        "delivery directory",
    )?;
    let package_canon = canon_dir(&layout.package_dir, "package directory")?;
    inside(
        &package_canon,
        &delivery_canon,
        &layout.package_dir,
        "package directory",
    )?;

    regular_file(&layout.manifest_path, "manifest")?;
    inside(
        &canon(&layout.manifest_path)?,
        &delivery_canon,
        &layout.manifest_path,
        "manifest",
    )?;
    let manifest = Manifest::read(&layout.manifest_path)?;
    let mut warnings: Vec<String> = manifest
        .validate(package_id)?
        .into_iter()
        .map(|(f, prefix)| format!("client-named file {f:?} allowed by client_named {prefix:?}"))
        .collect();

    // Every manifest file exists, is regular, not a symlink, and inside the package dir.
    for f in &manifest.files {
        let p = layout.package_dir.join(f);
        regular_file(&p, "manifest file")?;
        inside(&canon(&p)?, &package_canon, &p, "manifest file")?;
    }

    // Every file in the package dir is listed. No symlinks, no specials, no hidden.
    let listed: HashSet<&str> = manifest.files.iter().map(String::as_str).collect();
    let mut on_disk = BTreeSet::new();
    walk(&layout.package_dir, &layout.package_dir, &mut on_disk)?;
    for f in &on_disk {
        if !listed.contains(f.as_str()) {
            return Err(Error::UnsafePackage(format!(
                "{f:?} is in the package directory but not in the manifest"
            )));
        }
    }

    // The zip.
    regular_file(&layout.zip_path, "package zip")?;
    inside(
        &canon(&layout.zip_path)?,
        &delivery_canon,
        &layout.zip_path,
        "package zip",
    )?;
    check_zip(&layout.zip_path, &manifest.files)?;
    let (zip_sha256, zip_size) = sha256_of(&layout.zip_path)?;

    if !gitignore_covers_delivery(dev_path) {
        warnings.push("delivery/ is not ignored by .gitignore".into());
    }

    Ok(Checked {
        layout,
        kind: manifest.kind,
        files: manifest.files,
        zip_sha256,
        zip_size,
        warnings,
    })
}

pub fn sha256_of(path: &Path) -> Result<(String, u64)> {
    let mut f = fs::File::open(path).map_err(|e| Error::PathUnavailable(path.to_path_buf(), e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        hasher.update(&buf[..n]);
    }
    Ok((hex::encode(hasher.finalize()), size))
}

fn canon(p: &Path) -> Result<PathBuf> {
    fs::canonicalize(p).map_err(|e| Error::UnsafePackage(format!("{} ({e})", p.display())))
}

fn canon_dir(p: &Path, label: &str) -> Result<PathBuf> {
    let meta = fs::symlink_metadata(p)
        .map_err(|e| Error::UnsafePackage(format!("missing {label} {} ({e})", p.display())))?;
    if meta.file_type().is_symlink() {
        return Err(Error::UnsafePackage(format!(
            "{label} {} must not be a symlink",
            p.display()
        )));
    }
    if !meta.is_dir() {
        return Err(Error::UnsafePackage(format!(
            "{label} {} is not a directory",
            p.display()
        )));
    }
    canon(p)
}

fn inside(canonical: &Path, root: &Path, original: &Path, label: &str) -> Result<()> {
    if canonical.starts_with(root) {
        Ok(())
    } else {
        Err(Error::UnsafePackage(format!(
            "{label} {} resolves outside {}",
            original.display(),
            root.display()
        )))
    }
}

fn regular_file(p: &Path, label: &str) -> Result<()> {
    let meta = fs::symlink_metadata(p)
        .map_err(|e| Error::UnsafePackage(format!("missing {label} {} ({e})", p.display())))?;
    if meta.file_type().is_symlink() {
        return Err(Error::UnsafePackage(format!(
            "{label} {} must not be a symlink",
            p.display()
        )));
    }
    if !meta.is_file() {
        return Err(Error::UnsafePackage(format!(
            "{label} {} is not a regular file",
            p.display()
        )));
    }
    Ok(())
}

/// Collect every entry under `dir` as a forward-slash path relative to `root`.
/// Fails on the first symlink, special file or hidden entry.
pub(crate) fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| Error::PathUnavailable(dir.to_path_buf(), e))?
        .collect::<std::result::Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .expect("entry is under root")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let meta = fs::symlink_metadata(&path)?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            return Err(Error::UnsafePackage(format!("{rel:?} is a symlink")));
        }
        if ft.is_dir() {
            rules::hard(&rel)?;
            walk(root, &path, out)?;
        } else if ft.is_file() {
            rules::hard(&rel)?;
            out.insert(rel);
        } else {
            return Err(Error::UnsafePackage(format!(
                "{rel:?} is not a regular file"
            )));
        }
    }
    Ok(())
}

fn check_zip(zip_path: &Path, files: &[String]) -> Result<()> {
    let file =
        fs::File::open(zip_path).map_err(|e| Error::PathUnavailable(zip_path.to_path_buf(), e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::UnsafePackage(format!("invalid zip {} ({e})", zip_path.display())))?;
    let allowed: HashSet<&str> = files.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| Error::UnsafePackage(format!("invalid zip entry #{i} ({e})")))?;
        let raw = entry.name();
        let name = raw.trim_end_matches('/');
        rules::hard(name)?;
        if let Some(mode) = entry.unix_mode() {
            if mode & 0o170000 == 0o120000 {
                return Err(Error::UnsafePackage(format!(
                    "zip entry {raw:?} is a symlink"
                )));
            }
        }
        if entry.is_dir() {
            if !rules::is_parent_dir_of_any(name, files) {
                return Err(Error::UnsafePackage(format!(
                    "zip directory entry {raw:?} is not implied by the manifest"
                )));
            }
        } else {
            if !allowed.contains(name) {
                return Err(Error::UnsafePackage(format!(
                    "zip entry {name:?} is not in the manifest"
                )));
            }
            if !seen.insert(name.to_string()) {
                return Err(Error::UnsafePackage(format!(
                    "duplicate zip entry {name:?}"
                )));
            }
        }
    }
    for f in files {
        if !seen.contains(f) {
            return Err(Error::UnsafePackage(format!(
                "manifest file {f:?} is missing from the zip"
            )));
        }
    }
    Ok(())
}

pub(crate) fn gitignore_covers_delivery(dev_path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(dev_path.join(".gitignore")) else {
        return false;
    };
    text.lines().map(str::trim).any(|l| {
        matches!(
            l,
            "delivery" | "delivery/" | "/delivery" | "/delivery/" | "delivery/*" | "/delivery/*"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::build;
    use std::io::Write;

    fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let dev = dir.path().join("proj");
        let pkg = dev.join("delivery/p-v1");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(dev.join(".gitignore"), "delivery/\n").unwrap();
        for (name, content) in files {
            let p = pkg.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::File::create(&p)
                .unwrap()
                .write_all(content.as_bytes())
                .unwrap();
        }
        (dir, dev)
    }

    fn built(dev: &Path) -> Checked {
        build::build(dev, "p-v1", PackageKind::Full, true, &[]).unwrap()
    }

    #[test]
    fn good_package_passes() {
        let (_d, dev) = project(&[("manual.pdf", "x"), ("program/tool.exe", "y")]);
        let c = built(&dev);
        assert_eq!(c.files, vec!["manual.pdf", "program/tool.exe"]);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.zip_sha256.len(), 64);
        let again = check(&dev, "p-v1").unwrap();
        assert_eq!(again.zip_sha256, c.zip_sha256);
    }

    #[test]
    fn unlisted_file_is_rejected() {
        let (_d, dev) = project(&[("manual.pdf", "x")]);
        built(&dev);
        fs::write(dev.join("delivery/p-v1/extra.txt"), "z").unwrap();
        let e = check(&dev, "p-v1").unwrap_err();
        assert_eq!(e.code(), "unsafe_package");
        assert!(e.to_string().contains("extra.txt"));
    }

    #[test]
    fn hidden_and_internal_files_are_rejected_at_build() {
        let (_d, dev) = project(&[("manual.pdf", "x"), (".gig/JOB.md", "j")]);
        let e = build::build(&dev, "p-v1", PackageKind::Full, true, &[]).unwrap_err();
        assert_eq!(e.code(), "unsafe_package");
        let (_d, dev) = project(&[("manual.pdf", "x"), ("keys/server.pem", "k")]);
        assert!(build::build(&dev, "p-v1", PackageKind::Full, true, &[]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_rejected() {
        let (_d, dev) = project(&[("manual.pdf", "x")]);
        built(&dev);
        std::os::unix::fs::symlink(dev.join(".gitignore"), dev.join("delivery/p-v1/link.txt"))
            .unwrap();
        let e = check(&dev, "p-v1").unwrap_err();
        assert!(e.to_string().contains("symlink"), "{e}");
        fs::remove_file(dev.join("delivery/p-v1/link.txt")).unwrap();
        // manifest pointing at a symlink
        std::os::unix::fs::symlink(
            dev.join(".gitignore"),
            dev.join("delivery/p-v1/manual.pdf.lnk"),
        )
        .unwrap();
        let mut m = Manifest::read(&dev.join("delivery/p-v1.manifest.toml")).unwrap();
        m.files.push("manual.pdf.lnk".into());
        m.write(&dev.join("delivery/p-v1.manifest.toml")).unwrap();
        assert!(check(&dev, "p-v1").is_err());
    }

    #[test]
    fn zip_must_match_manifest_exactly() {
        let (_d, dev) = project(&[("manual.pdf", "x"), ("b.txt", "y")]);
        built(&dev);
        // drop one file from the manifest: zip now has an unmanifested entry
        let mp = dev.join("delivery/p-v1.manifest.toml");
        let mut m = Manifest::read(&mp).unwrap();
        m.files.retain(|f| f != "b.txt");
        m.write(&mp).unwrap();
        fs::remove_file(dev.join("delivery/p-v1/b.txt")).unwrap();
        let e = check(&dev, "p-v1").unwrap_err();
        assert!(e.to_string().contains("b.txt"), "{e}");
    }

    #[test]
    fn non_ascii_names_need_a_waiver() {
        let (_d, dev) = project(&[("manual.pdf", "x"), ("results/图 1.png", "y")]);
        assert!(build::build(&dev, "p-v1", PackageKind::Full, true, &[]).is_err());
        let mp = dev.join("delivery/p-v1.manifest.toml");
        Manifest {
            version: 1,
            package_id: "p-v1".into(),
            kind: PackageKind::Preview,
            files: vec!["manual.pdf".into(), "results/图 1.png".into()],
            client_named: vec!["results/".into()],
        }
        .write(&mp)
        .unwrap();
        let c = build::build(&dev, "p-v1", PackageKind::Preview, false, &[]).unwrap();
        assert_eq!(c.kind, PackageKind::Preview);
        assert!(c.warnings.iter().any(|w| w.contains("client-named")));
    }

    #[test]
    fn client_named_prefix_can_be_given_at_build_time() {
        let (_d, dev) = project(&[("manual.pdf", "x"), ("results/图 1.png", "y")]);
        let c = build::build(
            &dev,
            "p-v1",
            PackageKind::Full,
            true,
            &["results/".to_string()],
        )
        .unwrap();
        assert_eq!(c.files.len(), 2);
        assert!(c.warnings.iter().any(|w| w.contains("client-named")));
        let m = Manifest::read(&dev.join("delivery/p-v1.manifest.toml")).unwrap();
        assert_eq!(m.client_named, vec!["results/"]);
    }

    #[test]
    fn missing_gitignore_is_a_warning_not_an_error() {
        let (_d, dev) = project(&[("manual.pdf", "x")]);
        fs::remove_file(dev.join(".gitignore")).unwrap();
        let c = built(&dev);
        assert!(c.warnings.iter().any(|w| w.contains(".gitignore")));
    }

    #[test]
    fn manifest_outside_delivery_is_rejected() {
        let (_d, dev) = project(&[("manual.pdf", "x")]);
        built(&dev);
        fs::remove_file(dev.join("delivery/p-v1.manifest.toml")).unwrap();
        assert!(check(&dev, "p-v1").is_err());
    }
}
