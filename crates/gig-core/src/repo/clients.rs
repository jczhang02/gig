use crate::models::Client;
use crate::Result;
use rusqlite::{params, Connection, Row};

fn map_row(row: &Row<'_>) -> rusqlite::Result<Client> {
    Ok(Client {
        id: row.get("id")?,
        display_name: row.get("display_name")?,
        wechat_contact: row.get("wechat_contact")?,
        source_org: row.get("source_org")?,
        notes: row.get("notes")?,
        first_seen_at: row.get("first_seen_at")?,
    })
}

pub fn insert(
    conn: &Connection,
    display_name: &str,
    wechat_contact: Option<&str>,
    source_org: Option<&str>,
    notes: Option<&str>,
    first_seen_at: i64,
) -> Result<Client> {
    conn.execute(
        "INSERT INTO clients (display_name, wechat_contact, source_org, notes, first_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            display_name,
            wechat_contact,
            source_org,
            notes,
            first_seen_at
        ],
    )?;
    let id = conn.last_insert_rowid();
    find_by_id(conn, id)
}

pub fn find_by_id(conn: &Connection, id: i64) -> Result<Client> {
    let mut stmt = conn.prepare(
        "SELECT id, display_name, wechat_contact, source_org, notes, first_seen_at
         FROM clients WHERE id = ?1",
    )?;
    let client = stmt.query_row(params![id], map_row)?;
    Ok(client)
}

pub fn list(conn: &Connection) -> Result<Vec<Client>> {
    let mut stmt = conn.prepare(
        "SELECT id, display_name, wechat_contact, source_org, notes, first_seen_at
         FROM clients ORDER BY display_name",
    )?;
    let rows: Vec<Client> = stmt
        .query_map([], map_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn insert_and_find_roundtrip() {
        let conn = open_in_memory().unwrap();
        let c = insert(
            &conn,
            "Acme Corp",
            Some("acme-wx"),
            Some("org-1"),
            None,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(c.display_name, "Acme Corp");
        assert_eq!(c.wechat_contact.as_deref(), Some("acme-wx"));
        let again = find_by_id(&conn, c.id).unwrap();
        assert_eq!(again, c);
    }

    #[test]
    fn list_orders_by_display_name() {
        let conn = open_in_memory().unwrap();
        insert(&conn, "Bravo", None, None, None, 100).unwrap();
        insert(&conn, "Alfa", None, None, None, 200).unwrap();
        let all = list(&conn).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].display_name, "Alfa");
        assert_eq!(all[1].display_name, "Bravo");
    }
}
