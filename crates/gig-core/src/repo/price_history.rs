//! Price history repo.
use crate::models::PriceHistoryEntry;
use crate::Result;
use rusqlite::{params, Connection};

pub fn record(
    conn: &Connection,
    order_id: i64,
    old_price: Option<i64>,
    new_price: Option<i64>,
    reason: Option<&str>,
    created_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO price_history (order_id, old_price, new_price, reason, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![order_id, old_price, new_price, reason, created_at],
    )?;
    Ok(())
}

pub fn list_for_order(conn: &Connection, order_id: i64) -> Result<Vec<PriceHistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, order_id, old_price, new_price, reason, created_at
         FROM price_history WHERE order_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map(params![order_id], |row| {
            Ok(PriceHistoryEntry {
                id: row.get("id")?,
                order_id: row.get("order_id")?,
                old_price: row.get("old_price")?,
                new_price: row.get("new_price")?,
                reason: row.get("reason")?,
                created_at: row.get("created_at")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
