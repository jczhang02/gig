//! Tag and order_tags repository.

use crate::models::Tag;
use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};

fn map_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get("id")?,
        name: row.get("name")?,
    })
}

/// Find an existing tag by name, or insert it and return the new row.
pub fn find_or_create(conn: &Connection, name: &str) -> Result<Tag> {
    // Try to find first
    let mut stmt = conn.prepare("SELECT id, name FROM tags WHERE name = ?1")?;
    let existing = stmt
        .query_row(params![name], map_tag)
        .optional()
        .map_err(Error::Db)?;
    if let Some(tag) = existing {
        return Ok(tag);
    }
    conn.execute("INSERT INTO tags (name) VALUES (?1)", params![name])?;
    let id = conn.last_insert_rowid();
    Ok(Tag {
        id,
        name: name.to_string(),
    })
}

/// Attach a tag to an order. No-op if already attached.
pub fn attach(conn: &Connection, order_id: i64, tag_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO order_tags (order_id, tag_id) VALUES (?1, ?2)",
        params![order_id, tag_id],
    )?;
    Ok(())
}

/// Detach a tag from an order by tag name. No-op if not attached.
pub fn detach_by_name(conn: &Connection, order_id: i64, tag_name: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM order_tags WHERE order_id = ?1 AND tag_id = (
            SELECT id FROM tags WHERE name = ?2
         )",
        params![order_id, tag_name],
    )?;
    Ok(())
}

/// List all tags for an order.
pub fn list_for_order(conn: &Connection, order_id: i64) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name FROM tags t
         JOIN order_tags ot ON ot.tag_id = t.id
         WHERE ot.order_id = ?1
         ORDER BY t.name",
    )?;
    let rows = stmt
        .query_map(params![order_id], map_tag)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::OrderStatus;
    use crate::repo::orders::{insert, NewOrder};

    fn make_order(conn: &Connection) -> i64 {
        insert(
            conn,
            &NewOrder {
                slug: None,
                external_id: None,
                title: "test",
                client_id: None,
                source_org: None,
                source_id: None,
                status: OrderStatus::Accepted,
                quoted_price: None,
                final_price: None,
                my_cut_ratio: 0.6,
                currency: "CNY",
                notes: None,
                created_at: 0,
                accepted_at: None,
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn find_or_create_is_idempotent() {
        let conn = open_in_memory().unwrap();
        let t1 = find_or_create(&conn, "python").unwrap();
        let t2 = find_or_create(&conn, "python").unwrap();
        assert_eq!(t1.id, t2.id);
        assert_eq!(t1.name, "python");
    }

    #[test]
    fn attach_and_list() {
        let conn = open_in_memory().unwrap();
        let order_id = make_order(&conn);
        let tag = find_or_create(&conn, "rust").unwrap();
        attach(&conn, order_id, tag.id).unwrap();
        let tags = list_for_order(&conn, order_id).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "rust");
    }

    #[test]
    fn detach_removes_tag() {
        let conn = open_in_memory().unwrap();
        let order_id = make_order(&conn);
        let tag = find_or_create(&conn, "web").unwrap();
        attach(&conn, order_id, tag.id).unwrap();
        detach_by_name(&conn, order_id, "web").unwrap();
        let tags = list_for_order(&conn, order_id).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn attach_is_idempotent() {
        let conn = open_in_memory().unwrap();
        let order_id = make_order(&conn);
        let tag = find_or_create(&conn, "data").unwrap();
        attach(&conn, order_id, tag.id).unwrap();
        attach(&conn, order_id, tag.id).unwrap(); // should not error
        let tags = list_for_order(&conn, order_id).unwrap();
        assert_eq!(tags.len(), 1);
    }
}
