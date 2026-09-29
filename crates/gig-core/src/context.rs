//! Resolve the current order from the working directory.

use crate::models::Order;
use crate::repo::orders;
use crate::{Error, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub fn canonical(p: impl AsRef<Path>) -> Result<PathBuf> {
    let p = p.as_ref();
    std::fs::canonicalize(p).map_err(|e| Error::PathUnavailable(p.to_path_buf(), e))
}

/// True iff `haystack` is `needle` or a descendant of it. Both must be canonical.
/// Component-wise, so "foo" is not a prefix of "foo-bar".
pub fn is_within(haystack: &Path, needle: &Path) -> bool {
    let mut h = haystack.components();
    let mut n = needle.components();
    loop {
        match (h.next(), n.next()) {
            (Some(a), Some(b)) if a == b => continue,
            (_, Some(_)) => return false,
            (_, None) => return true,
        }
    }
}

/// The order whose dev_path or archive_path contains `cwd` (longest match wins).
pub fn resolve_for(conn: &Connection, cwd: &Path) -> Result<Option<Order>> {
    let mut best: Option<(i64, usize)> = None;
    for (id, dev, arch) in orders::paths(conn)? {
        for p in [dev, arch].into_iter().flatten() {
            let p = PathBuf::from(p);
            let p = std::fs::canonicalize(&p).unwrap_or(p);
            if is_within(cwd, &p) {
                let len = p.as_os_str().len();
                if best.is_none_or(|(_, l)| len > l) {
                    best = Some((id, len));
                }
            }
        }
    }
    match best {
        Some((id, _)) => Ok(Some(orders::find_by_id(conn, id)?)),
        None => Ok(None),
    }
}

/// `key` given -> resolve it; else the order for the current directory.
pub fn resolve_key_or_cwd(conn: &Connection, key: Option<&str>) -> Result<Order> {
    if let Some(k) = key {
        return orders::resolve(conn, k);
    }
    let cwd = canonical(std::env::current_dir()?)?;
    resolve_for(conn, &cwd)?.ok_or_else(|| {
        Error::NotFound(format!(
            "no order for {}; pass a slug or run inside a project directory",
            cwd.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_is_component_wise() {
        assert!(is_within(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(is_within(Path::new("/a/b"), Path::new("/a/b")));
        assert!(!is_within(Path::new("/a/b-c"), Path::new("/a/b")));
        assert!(!is_within(Path::new("/a"), Path::new("/a/b")));
    }
}
