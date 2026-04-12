//! Requirement change repo.
use crate::models::RequirementChange;
use crate::Result;
use rusqlite::{params, Connection};

pub fn add(
    conn: &Connection,
    order_id: i64,
    description: &str,
    price_delta: i64,
    created_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO requirement_changes (order_id, description, price_delta, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![order_id, description, price_delta, created_at],
    )?;
    Ok(())
}

pub fn list_for_order(conn: &Connection, order_id: i64) -> Result<Vec<RequirementChange>> {
    let mut stmt = conn.prepare(
        "SELECT id, order_id, description, price_delta, created_at
         FROM requirement_changes WHERE order_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map(params![order_id], |row| {
            Ok(RequirementChange {
                id: row.get("id")?,
                order_id: row.get("order_id")?,
                description: row.get("description")?,
                price_delta: row.get("price_delta")?,
                created_at: row.get("created_at")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
