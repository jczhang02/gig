//! Requirement change repo. Populated in Plan 2 when `gig change` lands.
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
