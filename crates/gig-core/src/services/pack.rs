//! Pack service: collect project files respecting .gitignore/.gigignore and
//! create a compressed archive via system `tar`/`zip` commands.

use crate::{Error, Result};
use ignore::WalkBuilder;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Archive format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackFormat {
    /// zip (widely supported, good default for Windows clients).
    Zip,
    /// tar + zstd compression.
    TarZst,
}

impl PackFormat {
    /// Parse from a config/CLI string ("zip" or "tar.zst").
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "zip" => Some(Self::Zip),
            "tar.zst" | "tarzst" => Some(Self::TarZst),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarZst => "tar.zst",
        }
    }
}

/// Returned by `pack_order` on success.
pub struct PackResult {
    pub archive_path: PathBuf,
    pub files_count: usize,
}

/// Collect files to pack from `project_dir`, respecting:
/// 1. Project `.gitignore` (via `ignore` crate — same logic as ripgrep/git).
/// 2. `.git/` — always excluded regardless of gitignore.
/// 3. `extra_ignore` patterns from global config `[pack]`.
/// 4. `.gigignore` file in the project root (same syntax as gitignore).
///
/// Returns paths relative to `project_dir`.
pub fn collect_files(project_dir: &Path, extra_ignore: &[String]) -> Result<Vec<PathBuf>> {
    let mut builder = WalkBuilder::new(project_dir);
    builder
        .hidden(false) // include dot-files (but gitignore still applies)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .ignore(false); // disable .ignore files (use only gitignore + overrides)

    // Add global extra_ignore patterns as an override file written to memory.
    // The ignore crate supports adding custom ignore globs via `add_custom_ignore_filename`
    // for filenames, but for inline patterns we use `overrides`.
    if !extra_ignore.is_empty() {
        let mut ob = ignore::overrides::OverrideBuilder::new(project_dir);
        for pat in extra_ignore {
            // Negate the pattern so that matched files are *excluded*.
            // A bare pattern like "*.env" should exclude; "!README.md" should include.
            // Users write gitignore-syntax: bare = exclude, !prefix = include.
            // In the override builder the sense is opposite (matches = keep), so we flip:
            let override_pat = if let Some(stripped) = pat.strip_prefix('!') {
                // User wants to force-include: keep the override as-is (no '!')
                stripped.to_string()
            } else {
                // User wants to exclude: negate in override sense
                format!("!{pat}")
            };
            ob.add(&override_pat).map_err(|e| {
                Error::Invalid(format!("invalid extra_ignore pattern {pat:?}: {e}"))
            })?;
        }
        builder.overrides(
            ob.build()
                .map_err(|e| Error::Invalid(format!("failed to build ignore overrides: {e}")))?,
        );
    }

    // Add .gigignore as an additional custom ignore file.
    builder.add_custom_ignore_filename(".gigignore");

    let mut files: Vec<PathBuf> = Vec::new();

    for result in builder.build() {
        let entry = result.map_err(|e| Error::Invalid(format!("walk error: {e}")))?;

        // Skip directories and the .git directory explicitly.
        let path = entry.path();
        if path.is_dir() {
            continue;
        }

        // Double-check: never include anything under .git/
        let rel = path
            .strip_prefix(project_dir)
            .map_err(|_| Error::Invalid("walk produced path outside project dir".into()))?;

        let first_component = rel
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned());
        if first_component.as_deref() == Some(".git") {
            continue;
        }

        files.push(rel.to_path_buf());
    }

    files.sort();
    Ok(files)
}

/// Create an archive at `output_path` containing all selected files from `project_dir`.
///
/// On `dry_run = true`, prints the file list and returns a result with `archive_path`
/// set to where the archive *would* have been written but does not actually create it.
pub fn pack_order(
    project_dir: &Path,
    output_path: &Path,
    format: &PackFormat,
    extra_ignore: &[String],
    dry_run: bool,
) -> Result<PackResult> {
    let files = collect_files(project_dir, extra_ignore)?;

    if dry_run {
        for f in &files {
            println!("{}", f.display());
        }
        return Ok(PackResult {
            archive_path: output_path.to_path_buf(),
            files_count: files.len(),
        });
    }

    if files.is_empty() {
        return Err(Error::Invalid(
            "no files selected for packing (all ignored?)".into(),
        ));
    }

    // Build a NUL-delimited file list for xargs-safe passing.
    let file_list: Vec<u8> = files
        .iter()
        .flat_map(|f| {
            let s = f.to_string_lossy();
            let mut v: Vec<u8> = s.as_bytes().to_vec();
            v.push(b'\0');
            v
        })
        .collect();

    match format {
        PackFormat::Zip => pack_zip(project_dir, output_path, &files, &file_list),
        PackFormat::TarZst => pack_tar_zst(project_dir, output_path, &files, &file_list),
    }
}

fn pack_zip(
    project_dir: &Path,
    output_path: &Path,
    files: &[PathBuf],
    _file_list: &[u8],
) -> Result<PackResult> {
    // zip -0 ... reads filenames from stdin with -@ (BSD zip) or uses args.
    // Most portable: pass filenames as arguments, but there may be many.
    // Use `zip output.zip file1 file2 ...` executed from project_dir.
    // For large file lists we write a temp file list and use `zip -@ < list`.

    let out = if output_path.is_absolute() {
        output_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(Error::Io)?
            .join(output_path)
    };

    // Remove existing archive so zip doesn't append.
    if out.exists() {
        std::fs::remove_file(&out).map_err(Error::Io)?;
    }

    // Write file list to zip via stdin (-@).
    // file list: one path per line (zip -@ reads newline-separated).
    let file_list_newline: Vec<u8> = files
        .iter()
        .flat_map(|f| {
            let s = f.to_string_lossy();
            let mut v: Vec<u8> = s.as_bytes().to_vec();
            v.push(b'\n');
            v
        })
        .collect();

    let mut child = Command::new("zip")
        .arg("-r")
        .arg(&out)
        .arg("-@")
        .current_dir(project_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Config(
                    "zip is not installed or not on PATH; install it (e.g. `apt install zip`)"
                        .into(),
                )
            } else {
                Error::Io(e)
            }
        })?;

    child
        .stdin
        .take()
        .unwrap()
        .write_all(&file_list_newline)
        .map_err(Error::Io)?;

    let status = child.wait().map_err(Error::Io)?;
    if !status.success() {
        return Err(Error::Invalid(format!(
            "zip failed with exit code {}",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(PackResult {
        archive_path: out,
        files_count: files.len(),
    })
}

fn pack_tar_zst(
    project_dir: &Path,
    output_path: &Path,
    files: &[PathBuf],
    _file_list: &[u8],
) -> Result<PackResult> {
    let out = if output_path.is_absolute() {
        output_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(Error::Io)?
            .join(output_path)
    };

    // tar --null -T - -cf - | zstd > output.tar.zst
    // We pipe file list via stdin using --null -T -.
    // NUL-delimited list for robustness with spaces/special chars.
    let file_list_null: Vec<u8> = files
        .iter()
        .flat_map(|f| {
            let s = f.to_string_lossy();
            let mut v: Vec<u8> = s.as_bytes().to_vec();
            v.push(b'\0');
            v
        })
        .collect();

    // Spawn tar, writing to stdout, reading file list from stdin.
    let mut tar_child = Command::new("tar")
        .args(["--null", "-T", "-", "-cf", "-"])
        .current_dir(project_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Config("tar is not installed or not on PATH".into())
            } else {
                Error::Io(e)
            }
        })?;

    // Feed file list to tar stdin.
    tar_child
        .stdin
        .take()
        .unwrap()
        .write_all(&file_list_null)
        .map_err(Error::Io)?;

    // Spawn zstd reading from tar stdout, writing to output file.
    let tar_stdout = tar_child.stdout.take().unwrap();

    let out_file =
        std::fs::File::create(&out).map_err(|e| Error::PathUnavailable(out.clone(), e))?;

    let mut zstd_child = Command::new("zstd")
        .args(["-q", "-o", "/dev/stdout"])
        .stdin(tar_stdout)
        .stdout(out_file)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Config(
                    "zstd is not installed or not on PATH; install it (e.g. `apt install zstd`)"
                        .into(),
                )
            } else {
                Error::Io(e)
            }
        })?;

    let tar_status = tar_child.wait().map_err(Error::Io)?;
    let zstd_status = zstd_child.wait().map_err(Error::Io)?;

    if !tar_status.success() {
        return Err(Error::Invalid(format!(
            "tar failed with exit code {}",
            tar_status.code().unwrap_or(-1)
        )));
    }
    if !zstd_status.success() {
        return Err(Error::Invalid(format!(
            "zstd failed with exit code {}",
            zstd_status.code().unwrap_or(-1)
        )));
    }

    Ok(PackResult {
        archive_path: out,
        files_count: files.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_project(dir: &Path) {
        // tracked files
        fs::write(dir.join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.join("README.md"), "# project").unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/lib.rs"), "pub fn foo() {}").unwrap();

        // ignored files via .gitignore
        fs::write(dir.join(".gitignore"), "target/\n*.log\n").unwrap();
        fs::create_dir_all(dir.join("target/debug")).unwrap();
        fs::write(dir.join("target/debug/bin"), "binary").unwrap();
        fs::write(dir.join("build.log"), "log output").unwrap();

        // .git dir — always excluded
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main").unwrap();
    }

    #[test]
    fn collect_files_excludes_gitignore_and_dot_git() {
        let tmp = TempDir::new().unwrap();
        make_project(tmp.path());

        let files = collect_files(tmp.path(), &[]).unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|f| f.to_string_lossy().into_owned())
            .collect();

        // Should include tracked files
        assert!(
            names.contains(&"main.rs".to_string()),
            "missing main.rs: {names:?}"
        );
        assert!(
            names.contains(&"README.md".to_string()),
            "missing README.md: {names:?}"
        );
        assert!(
            names.contains(&"src/lib.rs".to_string()),
            "missing src/lib.rs: {names:?}"
        );
        assert!(
            names.contains(&".gitignore".to_string()),
            "missing .gitignore: {names:?}"
        );

        // Should exclude gitignored dirs
        assert!(
            !names.iter().any(|n| n.starts_with("target/")),
            "target/ should be excluded: {names:?}"
        );
        assert!(
            !names.contains(&"build.log".to_string()),
            "build.log should be excluded: {names:?}"
        );

        // Should never include .git/
        assert!(
            !names.iter().any(|n| n.starts_with(".git/")),
            ".git/ should be excluded: {names:?}"
        );
    }

    #[test]
    fn collect_files_respects_extra_ignore() {
        let tmp = TempDir::new().unwrap();
        make_project(tmp.path());
        fs::write(tmp.path().join("secret.env"), "API_KEY=abc").unwrap();

        let files = collect_files(tmp.path(), &["*.env".to_string()]).unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|f| f.to_string_lossy().into_owned())
            .collect();

        assert!(
            !names.contains(&"secret.env".to_string()),
            "secret.env should be excluded by extra_ignore: {names:?}"
        );
    }

    #[test]
    fn collect_files_respects_gigignore() {
        let tmp = TempDir::new().unwrap();
        make_project(tmp.path());
        fs::write(tmp.path().join("private.txt"), "secret").unwrap();
        fs::write(tmp.path().join(".gigignore"), "private.txt\n").unwrap();

        let files = collect_files(tmp.path(), &[]).unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|f| f.to_string_lossy().into_owned())
            .collect();

        assert!(
            !names.contains(&"private.txt".to_string()),
            "private.txt should be excluded by .gigignore: {names:?}"
        );
    }

    #[test]
    fn pack_dry_run_prints_files_without_creating_archive() {
        let tmp = TempDir::new().unwrap();
        make_project(tmp.path());
        let out = tmp.path().join("out.zip");

        let result = pack_order(tmp.path(), &out, &PackFormat::Zip, &[], true).unwrap();

        assert_eq!(result.archive_path, out);
        assert!(result.files_count > 0);
        // dry_run must NOT create the archive
        assert!(!out.exists(), "archive should not be created on dry_run");
    }
}
