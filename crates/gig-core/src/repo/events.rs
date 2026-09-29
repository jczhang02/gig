use crate::models::Event;
use crate::Result;
use rusqlite::{params, Connection};

pub const CHECK_REJECTED: &str = "check_rejected";
pub const PREVIEW_SENT: &str = "preview_sent";

pub fn insert(
    conn: &Connection,
    order_id: i64,
    kind: &str,
    detail: Option<&str>,
    at: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO events (order_id, kind, detail, at) VALUES (?1, ?2, ?3, ?4)",
        params![order_id, kind, detail, at],
    )?;
    Ok(())
}

pub fn count(conn: &Connection, order_id: i64, kind: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT count(*) FROM events WHERE order_id = ?1 AND kind = ?2",
        params![order_id, kind],
        |r| r.get(0),
    )?)
}

pub fn first_at(conn: &Connection, order_id: i64, kind: &str) -> Result<Option<String>> {
    Ok(conn.query_row(
        "SELECT min(at) FROM events WHERE order_id = ?1 AND kind = ?2",
        params![order_id, kind],
        |r| r.get(0),
    )?)
}

pub fn list_for_order(conn: &Connection, order_id: i64) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT id, order_id, kind, detail, at FROM events WHERE order_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([order_id], |r| {
        Ok(Event {
            id: r.get(0)?,
            order_id: r.get(1)?,
            kind: r.get(2)?,
            detail: r.get(3)?,
            at: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
