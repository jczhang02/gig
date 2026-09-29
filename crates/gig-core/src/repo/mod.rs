//! SQL for each table. No business rules here; services own those.

pub mod artifacts;
pub mod drafts;
pub mod events;
pub mod orders;
pub mod packages;
pub mod scorecards;

use crate::models::{PriceHistory, RequirementChange};
use crate::Result;
use rusqlite::{params, Connection};

pub fn insert_price_history(
    conn: &Connection,
    order_id: i64,
    old_minor: Option<i64>,
    new_minor: Option<i64>,
    reason: Option<&str>,
    at: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO price_history (order_id, old_minor, new_minor, reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![order_id, old_minor, new_minor, reason, at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_price_history(conn: &Connection, order_id: i64) -> Result<Vec<PriceHistory>> {
    let mut stmt = conn.prepare(
        "SELECT id, order_id, old_minor, new_minor, reason, created_at FROM price_history WHERE order_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([order_id], |r| {
        Ok(PriceHistory {
            id: r.get(0)?,
            order_id: r.get(1)?,
            old_minor: r.get(2)?,
            new_minor: r.get(3)?,
            reason: r.get(4)?,
            created_at: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn insert_requirement_change(
    conn: &Connection,
    order_id: i64,
    description: &str,
    price_delta_minor: i64,
    at: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO requirement_changes (order_id, description, price_delta_minor, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![order_id, description, price_delta_minor, at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_requirement_changes(
    conn: &Connection,
    order_id: i64,
) -> Result<Vec<RequirementChange>> {
    let mut stmt = conn.prepare(
        "SELECT id, order_id, description, price_delta_minor, created_at FROM requirement_changes WHERE order_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([order_id], |r| {
        Ok(RequirementChange {
            id: r.get(0)?,
            order_id: r.get(1)?,
            description: r.get(2)?,
            price_delta_minor: r.get(3)?,
            created_at: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
