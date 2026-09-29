//! Path rules for package ids, manifest entries and zip entries.
//!
//! Hard rules never relax: they stop traversal, hidden and internal files, secrets
//! and anything a zip extractor could misuse. The naming convention (ASCII, no
//! spaces) can be waived per directory prefix for files whose names come from the
//! client; the waiver is reported as a warning so it stays visible.

use crate::{Error, Result};

const BANNED_COMPONENTS: &[&str] = &[
    "internal",
    "prompts",
    "node_modules",
    "__pycache__",
    "venv",
    ".venv",
    ".gig",
    ".git",
    ".scratch",
];

const BANNED_SUFFIXES: &[&str] = &[".pem", ".key", ".p12", ".pfx"];
const BANNED_PREFIXES: &[&str] = &[
    "id_rsa",
    "id_ed25519",
    "id_ecdsa",
    ".env",
    "secrets",
    "credentials",
];

pub fn validate_package_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && !id.contains("..")
        && !id.to_ascii_lowercase().ends_with(".zip");
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "invalid package id {id:?}: 1-64 ASCII letters, digits, '.', '_' or '-', starting with a letter or digit, no '..', no .zip suffix"
        )))
    }
}

/// Outcome of the naming-convention check for one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Convention {
    Ok,
    /// Exempted by a `client_named` prefix; the string is the prefix.
    Exempted(String),
}

/// Apply the hard rules, then the naming convention.
pub fn check_path(path: &str, client_named: &[String]) -> Result<Convention> {
    hard(path)?;
    if conforms(path) {
        return Ok(Convention::Ok);
    }
    for prefix in client_named {
        if path.starts_with(prefix.as_str()) {
            return Ok(Convention::Exempted(prefix.clone()));
        }
    }
    Err(Error::UnsafePackage(format!(
        "{path:?} does not follow the naming convention (ASCII letters, digits, '.', '_', '-', '/'; components start with a letter or digit); list its directory in manifest client_named to allow client-derived names"
    )))
}

fn unsafe_path<T>(path: &str, why: &str) -> Result<T> {
    Err(Error::UnsafePackage(format!("{path:?}: {why}")))
}

pub(crate) fn hard(path: &str) -> Result<()> {
    if path.is_empty() {
        return unsafe_path(path, "empty path");
    }
    if path.contains('\\') {
        return unsafe_path(path, "backslash in path");
    }
    if path.starts_with('/') {
        return unsafe_path(path, "absolute path");
    }
    if path.chars().any(|c| c.is_control()) {
        return unsafe_path(path, "control character in path");
    }
    if path.ends_with('/') {
        return unsafe_path(path, "trailing slash");
    }
    let components: Vec<&str> = path.split('/').collect();
    for c in &components {
        if c.is_empty() {
            return unsafe_path(path, "empty path component");
        }
        if *c == "." || *c == ".." {
            return unsafe_path(path, "dot or dot-dot component");
        }
        if c.starts_with('.') {
            return unsafe_path(path, "hidden component");
        }
        if c.len() > 1 && c.as_bytes()[1] == b':' && c.as_bytes()[0].is_ascii_alphabetic() {
            return unsafe_path(path, "drive letter");
        }
        let lower = c.to_ascii_lowercase();
        if BANNED_COMPONENTS.contains(&lower.as_str()) {
            return unsafe_path(path, &format!("internal or workflow component {c:?}"));
        }
    }
    let base = components
        .last()
        .expect("non-empty split")
        .to_ascii_lowercase();
    if BANNED_SUFFIXES.iter().any(|s| base.ends_with(s)) {
        return unsafe_path(path, "looks like a private key");
    }
    if BANNED_PREFIXES.iter().any(|p| base.starts_with(p)) {
        return unsafe_path(path, "looks like a secret or credential file");
    }
    if base == "config.toml" && components.iter().any(|c| c.eq_ignore_ascii_case("gig")) {
        return unsafe_path(path, "gig config file");
    }
    Ok(())
}

fn conforms(path: &str) -> bool {
    path.split('/').all(|c| {
        c.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
            && c.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    })
}

/// Zip directory entries are allowed only when some file lives under them.
pub fn is_parent_dir_of_any(name: &str, files: &[String]) -> bool {
    let prefix = format!("{name}/");
    files.iter().any(|f| f.starts_with(&prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(p: &str) {
        assert_eq!(check_path(p, &[]).unwrap(), Convention::Ok, "{p}");
    }
    fn bad(p: &str) {
        let e = check_path(p, &[]).unwrap_err();
        assert_eq!(e.code(), "unsafe_package", "{p}: {e}");
    }

    #[test]
    fn accepts_plain_names() {
        for p in [
            "manual.pdf",
            "program/tool.exe",
            "results/1-234.png",
            "a/b/c.txt",
            "v1.2_x-y.zip",
        ] {
            ok(p);
        }
    }

    #[test]
    fn hard_rules() {
        for p in [
            "",
            "../x",
            "a/../b",
            "./a",
            "/etc/passwd",
            "a\\b",
            ".hidden",
            "a/.hidden/b",
            ".gig/JOB.md",
            "a/.git/config",
            "internal/notes.md",
            "prompts/x.md",
            "a/node_modules/b",
            "id_rsa",
            "keys/server.pem",
            "secrets.toml",
            ".env.production",
            "gig/config.toml",
            "a/",
            "a//b",
            "C:/x",
            "a\u{7}b",
        ] {
            bad(p);
        }
    }

    #[test]
    fn convention_can_be_waived_per_prefix() {
        assert!(check_path("results/图.png", &[]).is_err());
        assert!(check_path("results/a b.png", &[]).is_err());
        assert_eq!(
            check_path("results/图.png", &["results/".into()]).unwrap(),
            Convention::Exempted("results/".into())
        );
        // waiver never relaxes hard rules
        assert!(check_path("results/../x.png", &["results/".into()]).is_err());
        assert!(check_path("results/.hidden", &["results/".into()]).is_err());
    }

    #[test]
    fn package_ids() {
        for id in ["tk-dtf-compact-v1.1.0", "a", "A1_b.c"] {
            validate_package_id(id).unwrap();
        }
        for id in [
            "",
            "-a",
            ".a",
            "a..b",
            "a.zip",
            "a b",
            "中文",
            &"x".repeat(65),
        ] {
            assert!(validate_package_id(id).is_err(), "{id}");
        }
    }
}
