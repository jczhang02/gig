use crate::models::Artifact;
use crate::Result;
use rusqlite::{params, Connection, Row};

const COLS: &str =
    "id, order_id, local_path, uploader, remote_url, short_url, expires_at, uploaded_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Artifact> {
    Ok(Artifact {
        id: r.get(0)?,
        order_id: r.get(1)?,
        local_path: r.get(2)?,
        uploader: r.get(3)?,
        remote_url: r.get(4)?,
        short_url: r.get(5)?,
        expires_at: r.get(6)?,
        uploaded_at: r.get(7)?,
    })
}

pub struct NewArtifact<'a> {
    pub order_id: i64,
    pub local_path: Option<&'a str>,
    pub uploader: Option<&'a str>,
    pub remote_url: Option<&'a str>,
    pub short_url: Option<&'a str>,
    pub expires_at: Option<&'a str>,
    pub uploaded_at: &'a str,
}

pub fn insert(conn: &Connection, n: &NewArtifact<'_>) -> Result<Artifact> {
    conn.execute(
        "INSERT INTO artifacts (order_id, local_path, uploader, remote_url, short_url, expires_at, uploaded_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![n.order_id, n.local_path, n.uploader, n.remote_url, n.short_url, n.expires_at, n.uploaded_at],
    )?;
    let id = conn.last_insert_rowid();
    Ok(conn.query_row(
        &format!("SELECT {COLS} FROM artifacts WHERE id = ?1"),
        [id],
        from_row,
    )?)
}

pub fn list_for_order(conn: &Connection, order_id: i64) -> Result<Vec<Artifact>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM artifacts WHERE order_id = ?1 ORDER BY id"
    ))?;
    let rows = stmt.query_map([order_id], from_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
