use crate::models::Source;
use crate::Result;
use rusqlite::{params, Connection, Row};

fn map_row(row: &Row<'_>) -> rusqlite::Result<Source> {
    Ok(Source {
        id: row.get("id")?,
        name: row.get("name")?,
        cut_ratio: row.get("cut_ratio")?,
        notes: row.get("notes")?,
    })
}

pub fn insert(
    conn: &Connection,
    name: &str,
    cut_ratio: f64,
    notes: Option<&str>,
) -> Result<Source> {
    conn.execute(
        "INSERT INTO sources (name, cut_ratio, notes) VALUES (?1, ?2, ?3)",
        params![name, cut_ratio, notes],
    )?;
    find_by_id(conn, conn.last_insert_rowid())
}

pub fn find_by_id(conn: &Connection, id: i64) -> Result<Source> {
    let mut stmt =
        conn.prepare("SELECT id, name, cut_ratio, notes FROM sources WHERE id = ?1")?;
    let source = stmt.query_row(params![id], map_row)?;
    Ok(source)
}

pub fn find_by_name(conn: &Connection, name: &str) -> Result<Option<Source>> {
    let mut stmt =
        conn.prepare("SELECT id, name, cut_ratio, notes FROM sources WHERE name = ?1 LIMIT 1")?;
    let mut rows = stmt.query_map(params![name], map_row)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn list(conn: &Connection) -> Result<Vec<Source>> {
    let mut stmt =
        conn.prepare("SELECT id, name, cut_ratio, notes FROM sources ORDER BY name")?;
    let rows: Vec<Source> = stmt
        .query_map([], map_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn find_or_create(
    conn: &Connection,
    name: &str,
    cut_ratio: f64,
    notes: Option<&str>,
) -> Result<Source> {
    if let Some(existing) = find_by_name(conn, name)? {
        return Ok(existing);
    }
    insert(conn, name, cut_ratio, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn insert_and_find_roundtrip() {
        let conn = open_in_memory().unwrap();
        let s = insert(&conn, "PlatformA", 0.6, Some("some notes")).unwrap();
        assert_eq!(s.name, "PlatformA");
        assert!((s.cut_ratio - 0.6).abs() < 1e-9);
        assert_eq!(s.notes.as_deref(), Some("some notes"));
        let again = find_by_id(&conn, s.id).unwrap();
        assert_eq!(again, s);
    }

    #[test]
    fn find_by_name_returns_none_for_missing() {
        let conn = open_in_memory().unwrap();
        assert!(find_by_name(&conn, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn find_or_create_returns_existing() {
        let conn = open_in_memory().unwrap();
        let first = find_or_create(&conn, "PlatformB", 0.7, None).unwrap();
        let second = find_or_create(&conn, "PlatformB", 0.8, None).unwrap();
        // Should return existing with original cut_ratio, not 0.8
        assert_eq!(first.id, second.id);
        assert!((second.cut_ratio - 0.7).abs() < 1e-9);
    }

    #[test]
    fn list_returns_all_sorted() {
        let conn = open_in_memory().unwrap();
        insert(&conn, "Zebra", 0.5, None).unwrap();
        insert(&conn, "Alpha", 0.6, None).unwrap();
        let all = list(&conn).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "Alpha");
        assert_eq!(all[1].name, "Zebra");
    }
}
