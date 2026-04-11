//! Price history repo. Populated in Plan 2 when `gig price` lands.
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
