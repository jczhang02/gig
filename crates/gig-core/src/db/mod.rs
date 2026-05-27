//! SQLite connection + migrations.
//!
//! The only module in gig-core that opens a rusqlite Connection.

use crate::{Error, Result};
use rusqlite::Connection;
use std::path::Path;

refinery::embed_migrations!("src/db/migrations");

/// Open a connection to the gig database at `path`, running pending migrations.
///
/// Creates the parent directory if missing. Enables foreign keys.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::PathUnavailable(parent.to_path_buf(), e))?;
    }
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrations::runner().run(&mut conn)?;
    Ok(conn)
}

/// Open an in-memory database (for tests).
#[doc(hidden)]
pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrations::runner().run(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_has_expected_tables() {
        let conn = open_in_memory().unwrap();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .filter(|n| !n.starts_with("refinery") && !n.starts_with("sqlite_"))
            .collect();

        assert!(tables.contains(&"clients".to_string()));
        assert!(tables.contains(&"orders".to_string()));
        assert!(tables.contains(&"price_history".to_string()));
        assert!(tables.contains(&"requirement_changes".to_string()));
        assert!(tables.contains(&"delivery_artifacts".to_string()));
        assert!(tables.contains(&"tags".to_string()));
        assert!(tables.contains(&"order_tags".to_string()));
    }

    #[test]
    fn workflow_support_migration_creates_tables_and_orders_project_type() {
        let conn = open_in_memory().unwrap();

        for table in ["quote_drafts", "order_workflow", "delivery_packages"] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "missing workflow support table {table}");
        }

        let project_type_cols: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('orders') WHERE name = 'project_type'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(project_type_cols, 1, "orders.project_type should exist");
    }

    #[test]
    fn foreign_keys_are_enabled() {
        let conn = open_in_memory().unwrap();
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn idempotent_migration_run() {
        // Running the migration a second time on the same DB should be a no-op.
        let conn = open_in_memory().unwrap();
        let mut conn2 = conn;
        migrations::runner().run(&mut conn2).unwrap();
    }
}
