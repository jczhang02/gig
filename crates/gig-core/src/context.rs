//! Context awareness: resolve the current order from the working directory.
//!
//! See spec §5.7 for invariants and algorithm.

use crate::{Error, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Canonicalise a path (resolve symlinks, make absolute, strip trailing slash).
pub fn canonical(p: impl AsRef<Path>) -> Result<PathBuf> {
    std::fs::canonicalize(p).map_err(Error::from)
}

/// True iff `haystack` is `needle` or a descendant directory of `needle`.
/// Both inputs MUST already be canonical.
pub fn is_within(haystack: &Path, needle: &Path) -> bool {
    if haystack == needle {
        return true;
    }
    // Walk component-by-component instead of string prefix — immune to
    // "foo" vs "foo-bar" false positives.
    let mut h = haystack.components();
    let mut n = needle.components();
    loop {
        match (h.next(), n.next()) {
            (Some(a), Some(b)) if a == b => continue,
            (_, Some(_)) => return false, // needle still has components → not a prefix
            (Some(_), None) => return true, // needle exhausted, haystack still has more → descendant
            (None, None) => return true,    // exact equality (already handled above, but safe)
        }
    }
}

/// Resolve the current order from `std::env::current_dir()`.
///
/// Delegates to [`resolve_context_for`] with the canonicalised CWD.
pub fn resolve_context(conn: &Connection) -> Result<Option<i64>> {
    let cwd = canonical(std::env::current_dir()?)?;
    resolve_context_for(conn, &cwd)
}

/// Resolve the current order for a given (already-canonical) working directory.
///
/// Pulls all orders with `dev_path` or `archive_path`, does component-wise
/// prefix matching in Rust (NOT SQL LIKE), returns the id of the longest match.
pub fn resolve_context_for(conn: &Connection, cwd: &Path) -> Result<Option<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id, dev_path, archive_path FROM orders
         WHERE dev_path IS NOT NULL OR archive_path IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;

    let mut best: Option<(i64, usize)> = None; // (id, matched path length)
    for row in rows {
        let (id, dev, arch) = row?;
        for path_str in [dev, arch].into_iter().flatten() {
            let p = PathBuf::from(&path_str);
            if is_within(cwd, &p) {
                let len = p.as_os_str().len();
                if best.map_or(true, |(_, best_len)| len > best_len) {
                    best = Some((id, len));
                }
            }
        }
    }
    Ok(best.map(|(id, _)| id))
}

/// Check path consistency for all orders (used by `gig doctor`).
///
/// Returns a list of diagnostic messages.
pub fn doctor_path_checks(conn: &Connection) -> Result<Vec<String>> {
    let mut diagnostics = Vec::new();

    let mut stmt = conn.prepare(
        "SELECT id, slug, dev_path, archive_path FROM orders
         WHERE dev_path IS NOT NULL OR archive_path IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;

    // Collect (id, slug, path_kind, stored_path) tuples for nesting check.
    let mut canonical_paths: Vec<(i64, String, PathBuf)> = Vec::new();

    for row in rows {
        let (id, slug, dev, arch) = row?;
        let label = slug.unwrap_or_else(|| format!("#{id}"));

        for (kind, path_str) in [("dev_path", dev), ("archive_path", arch)] {
            if let Some(ref s) = path_str {
                let p = PathBuf::from(s);
                match std::fs::canonicalize(&p) {
                    Ok(canon) => {
                        if canon != p {
                            diagnostics.push(format!(
                                "drift: order #{id} ({label}) {kind} stored={} canonical={}",
                                p.display(),
                                canon.display()
                            ));
                        }
                        canonical_paths.push((id, format!("{label}/{kind}"), canon));
                    }
                    Err(e) => {
                        diagnostics.push(format!(
                            "missing: order #{id} ({label}) {kind}={} ({})",
                            p.display(),
                            e
                        ));
                    }
                }
            }
        }
    }

    // Check for nesting: any two paths where one is ancestor of the other.
    for i in 0..canonical_paths.len() {
        for j in (i + 1)..canonical_paths.len() {
            let (id_a, ref label_a, ref path_a) = canonical_paths[i];
            let (id_b, ref label_b, ref path_b) = canonical_paths[j];
            if id_a != id_b && (is_within(path_a, path_b) || is_within(path_b, path_a)) {
                diagnostics.push(format!(
                    "nesting: {} ({}) and {} ({}) are nested",
                    label_a,
                    path_a.display(),
                    label_b,
                    path_b.display()
                ));
            }
        }
    }

    Ok(diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_within_exact_match() {
        let p = Path::new("/home/jc/dev/acme");
        assert!(is_within(p, p));
    }

    #[test]
    fn is_within_true_subdirectory() {
        let parent = Path::new("/home/jc/dev/acme");
        let child = Path::new("/home/jc/dev/acme/src/main.rs");
        assert!(is_within(child, parent));
    }

    #[test]
    fn is_within_rejects_similar_prefix() {
        let needle = Path::new("/tmp/acme");
        let haystack = Path::new("/tmp/acme-scraper");
        assert!(!is_within(haystack, needle));
    }

    #[test]
    fn is_within_unrelated_paths() {
        let a = Path::new("/home/jc/dev/foo");
        let b = Path::new("/home/jc/other/bar");
        assert!(!is_within(a, b));
    }
}
