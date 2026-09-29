//! SQLite connection and migrations. The only module that opens a Connection.

use crate::{Error, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

refinery::embed_migrations!("src/db/migrations");

/// Open (creating if needed) the v2 database and run pending migrations.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::PathUnavailable(parent.to_path_buf(), e))?;
    }
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    if is_legacy(&conn)? {
        return Err(Error::LegacyDb(format!(
            "{} is a gig v1 database; run gig migrate --from <it> to create the v2 database",
            path.display()
        )));
    }
    migrations::runner().run(&mut conn)?;
    Ok(conn)
}

/// In-memory database for tests.
pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrations::runner().run(&mut conn)?;
    Ok(conn)
}

/// Open a v1 database read-only (for `gig migrate`).
pub fn open_legacy_readonly(path: &Path) -> Result<Connection> {
    if !path.is_file() {
        return Err(Error::NotFound(format!(
            "legacy database {}",
            path.display()
        )));
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if !is_legacy(&conn)? {
        return Err(Error::InvalidInput(format!(
            "{} does not look like a gig v1 database (no order_workflow table)",
            path.display()
        )));
    }
    Ok(conn)
}

pub fn is_legacy(conn: &Connection) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='order_workflow'",
        [],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply() {
        let conn = open_in_memory().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('orders','drafts','packages','artifacts','scorecards','events','price_history','requirement_changes')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 8);
    }

    #[test]
    fn refuses_legacy_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("gig.db");
        {
            let c = Connection::open(&p).unwrap();
            c.execute_batch("CREATE TABLE order_workflow (order_id INTEGER PRIMARY KEY);")
                .unwrap();
        }
        assert_eq!(open(&p).unwrap_err().code(), "legacy_db");
        assert!(open_legacy_readonly(&p).is_ok());
    }
}
