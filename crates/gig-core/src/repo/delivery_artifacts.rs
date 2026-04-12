//! Repository for `delivery_artifacts` table.

use crate::models::DeliveryArtifact;
use crate::Result;
use rusqlite::{params, Connection, Row};

fn map_row(row: &Row<'_>) -> rusqlite::Result<DeliveryArtifact> {
    Ok(DeliveryArtifact {
        id: row.get("id")?,
        order_id: row.get("order_id")?,
        local_path: row.get("local_path")?,
        uploader_name: row.get("uploader_name")?,
        remote_url: row.get("remote_url")?,
        expires_at: row.get("expires_at")?,
        uploaded_at: row.get("uploaded_at")?,
    })
}

/// Insert a new delivery artifact and return it with the assigned id.
pub fn insert(
    conn: &Connection,
    order_id: i64,
    local_path: Option<&str>,
    uploader_name: Option<&str>,
    remote_url: Option<&str>,
    expires_at: Option<i64>,
    uploaded_at: i64,
) -> Result<DeliveryArtifact> {
    conn.execute(
        "INSERT INTO delivery_artifacts \
         (order_id, local_path, uploader_name, remote_url, expires_at, uploaded_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            order_id,
            local_path,
            uploader_name,
            remote_url,
            expires_at,
            uploaded_at
        ],
    )?;
    let id = conn.last_insert_rowid();
    find_by_id(conn, id)
}

pub fn find_by_id(conn: &Connection, id: i64) -> Result<DeliveryArtifact> {
    Ok(conn.query_row(
        "SELECT id, order_id, local_path, uploader_name, remote_url, expires_at, uploaded_at \
         FROM delivery_artifacts WHERE id = ?1",
        params![id],
        map_row,
    )?)
}

/// Return all delivery artifacts for an order, newest first.
pub fn list_for_order(conn: &Connection, order_id: i64) -> Result<Vec<DeliveryArtifact>> {
    let mut stmt = conn.prepare(
        "SELECT id, order_id, local_path, uploader_name, remote_url, expires_at, uploaded_at \
         FROM delivery_artifacts WHERE order_id = ?1 ORDER BY uploaded_at DESC",
    )?;
    let rows = stmt.query_map(params![order_id], map_row)?;
    rows.map(|r| r.map_err(Into::into)).collect()
}

/// Return the most-recent artifact for an order (for --resend).
pub fn latest_for_order(conn: &Connection, order_id: i64) -> Result<Option<DeliveryArtifact>> {
    let mut stmt = conn.prepare(
        "SELECT id, order_id, local_path, uploader_name, remote_url, expires_at, uploaded_at \
         FROM delivery_artifacts WHERE order_id = ?1 ORDER BY uploaded_at DESC LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![order_id], map_row)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}
