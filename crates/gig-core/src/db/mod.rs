//! SQLite connection + migrations.
//!
//! The only module in gig-core that opens a rusqlite Connection.

use crate::{Error, Result};
use rusqlite::{Connection, OpenFlags};
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

/// Open an existing database for read-only GUI requests.
///
/// Unlike [`open`], this does not create directories, create the database file,
/// or run migrations. The GUI is read-only, so missing or stale databases should
/// surface as explicit errors instead of changing local state.
pub fn open_readonly(path: &Path) -> Result<Connection> {
    if !path.exists() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "database file does not exist");
        return Err(Error::PathUnavailable(path.to_path_buf(), err));
    }

    let uri = sqlite_readonly_uri(path, !wal_sidecar_path(path).exists());
    let conn = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "query_only", "ON")?;
    Ok(conn)
}

fn sqlite_readonly_uri(path: &Path, immutable: bool) -> String {
    let path = path.to_string_lossy();
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    if immutable {
        format!("file:{encoded}?mode=ro&immutable=1")
    } else {
        format!("file:{encoded}?mode=ro")
    }
}

fn wal_sidecar_path(path: &Path) -> std::path::PathBuf {
    path.with_file_name(format!(
        "{}-wal",
        path.file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default()
    ))
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
    use crate::models::OrderStatus;
    use crate::repo::orders::{self, NewOrder};

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

        let work_started_cols: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('order_workflow') WHERE name = 'work_started_at'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            work_started_cols, 1,
            "order_workflow.work_started_at should exist"
        );
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
    fn readonly_open_requires_existing_database() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("missing/gig.db");

        let err = open_readonly(&path).unwrap_err();

        assert!(!path.exists());
        assert!(matches!(err, Error::PathUnavailable(_, _)));
    }

    #[test]
    fn readonly_open_rejects_writes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("gig.db");
        drop(open(&path).unwrap());

        let conn = open_readonly(&path).unwrap();
        let err = conn
            .execute(
                "INSERT INTO clients (display_name, first_seen_at) VALUES (?1, ?2)",
                ("client", 1_i64),
            )
            .unwrap_err()
            .to_string();

        assert!(err.contains("readonly") || err.contains("query only"));
    }

    #[test]
    fn readonly_open_reads_active_wal_data() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("gig.db");
        let writer = open(&path).unwrap();
        writer
            .pragma_update(None, "wal_autocheckpoint", 0_i64)
            .unwrap();
        let order = orders::insert(
            &writer,
            &NewOrder {
                slug: Some("wal-order"),
                external_id: None,
                title: "WAL order",
                client_id: None,
                source_org: None,
                source_id: None,
                project_type: None,
                status: OrderStatus::Lead,
                quoted_price: None,
                final_price: None,
                my_cut_ratio: 1.0,
                currency: "CNY",
                notes: None,
                created_at: 1,
                accepted_at: None,
            },
        )
        .unwrap();
        assert!(
            wal_sidecar_path(&path).exists(),
            "writer should keep active WAL"
        );

        let reader = open_readonly(&path).unwrap();
        let title: String = reader
            .query_row(
                "SELECT title FROM orders WHERE id = ?1",
                (order.id,),
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(title, "WAL order");
    }

    #[test]
    fn readonly_open_does_not_create_wal_sidecars_for_clean_database() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("gig.db");
        drop(open(&path).unwrap());
        let wal = tmp.path().join("gig.db-wal");
        let shm = tmp.path().join("gig.db-shm");
        let _ = std::fs::remove_file(&wal);
        let _ = std::fs::remove_file(&shm);

        let conn = open_readonly(&path).unwrap();
        let _: i64 = conn
            .query_row("SELECT COUNT(*) FROM orders", [], |row| row.get(0))
            .unwrap();
        drop(conn);

        assert!(!wal.exists(), "read-only open created WAL sidecar");
        assert!(!shm.exists(), "read-only open created SHM sidecar");
    }

    #[test]
    fn idempotent_migration_run() {
        // Running the migration a second time on the same DB should be a no-op.
        let conn = open_in_memory().unwrap();
        let mut conn2 = conn;
        migrations::runner().run(&mut conn2).unwrap();
    }
}
