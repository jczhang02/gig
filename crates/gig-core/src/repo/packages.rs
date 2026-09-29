use crate::models::{Channel, Package, PackageKind, PackageStatus};
use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};

const COLS: &str = "id, order_id, package_id, kind, dir, manifest_path, zip_path, zip_sha256, file_count, status, checked_at, sent_at, channel, uploader, remote_url, short_url, expires_at, created_at, updated_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Package> {
    let conv = |e: Error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    };
    let kind: String = r.get(3)?;
    let status: String = r.get(9)?;
    let channel: Option<String> = r.get(12)?;
    Ok(Package {
        id: r.get(0)?,
        order_id: r.get(1)?,
        package_id: r.get(2)?,
        kind: PackageKind::parse(&kind).map_err(conv)?,
        dir: r.get(4)?,
        manifest_path: r.get(5)?,
        zip_path: r.get(6)?,
        zip_sha256: r.get(7)?,
        file_count: r.get(8)?,
        status: PackageStatus::parse(&status).map_err(conv)?,
        checked_at: r.get(10)?,
        sent_at: r.get(11)?,
        channel: channel
            .map(|c| Channel::parse(&c))
            .transpose()
            .map_err(conv)?,
        uploader: r.get(13)?,
        remote_url: r.get(14)?,
        short_url: r.get(15)?,
        expires_at: r.get(16)?,
        created_at: r.get(17)?,
        updated_at: r.get(18)?,
    })
}

pub struct NewPackage<'a> {
    pub order_id: i64,
    pub package_id: &'a str,
    pub kind: PackageKind,
    pub dir: &'a str,
    pub manifest_path: &'a str,
    pub zip_path: &'a str,
    pub zip_sha256: Option<&'a str>,
    pub file_count: Option<i64>,
    pub status: PackageStatus,
    pub checked_at: Option<&'a str>,
    pub sent_at: Option<&'a str>,
    pub channel: Option<Channel>,
    pub uploader: Option<&'a str>,
    pub remote_url: Option<&'a str>,
    pub short_url: Option<&'a str>,
    pub expires_at: Option<&'a str>,
    pub created_at: &'a str,
}

/// Insert or replace the row for (order_id, package_id).
pub fn upsert(conn: &Connection, n: &NewPackage<'_>) -> Result<Package> {
    conn.execute(
        "INSERT INTO packages (order_id, package_id, kind, dir, manifest_path, zip_path, zip_sha256, file_count, status, checked_at, sent_at, channel, uploader, remote_url, short_url, expires_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)
         ON CONFLICT(order_id, package_id) DO UPDATE SET
           kind = excluded.kind, dir = excluded.dir, manifest_path = excluded.manifest_path,
           zip_path = excluded.zip_path, zip_sha256 = excluded.zip_sha256, file_count = excluded.file_count,
           status = excluded.status, checked_at = excluded.checked_at, sent_at = excluded.sent_at,
           channel = excluded.channel, uploader = excluded.uploader, remote_url = excluded.remote_url,
           short_url = excluded.short_url, expires_at = excluded.expires_at, updated_at = excluded.updated_at",
        params![
            n.order_id, n.package_id, n.kind.as_str(), n.dir, n.manifest_path, n.zip_path,
            n.zip_sha256, n.file_count, n.status.as_str(), n.checked_at, n.sent_at,
            n.channel.map(|c| c.as_str()), n.uploader, n.remote_url, n.short_url, n.expires_at,
            n.created_at
        ],
    )?;
    find(conn, n.order_id, n.package_id)?
        .ok_or_else(|| Error::NotFound(format!("package {}", n.package_id)))
}

pub fn find(conn: &Connection, order_id: i64, package_id: &str) -> Result<Option<Package>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM packages WHERE order_id = ?1 AND package_id = ?2"),
            params![order_id, package_id],
            from_row,
        )
        .optional()?)
}

pub fn list_for_order(conn: &Connection, order_id: i64) -> Result<Vec<Package>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM packages WHERE order_id = ?1 ORDER BY id"
    ))?;
    let rows = stmt.query_map([order_id], from_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn list_all(conn: &Connection) -> Result<Vec<Package>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM packages ORDER BY id"))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub struct Sent<'a> {
    pub sent_at: &'a str,
    pub channel: Channel,
    pub uploader: Option<&'a str>,
    pub remote_url: Option<&'a str>,
    pub short_url: Option<&'a str>,
    pub expires_at: Option<&'a str>,
}

pub fn mark_sent(conn: &Connection, id: i64, s: &Sent<'_>) -> Result<()> {
    conn.execute(
        "UPDATE packages SET status = 'sent', sent_at = ?2, channel = ?3, uploader = ?4, remote_url = ?5, short_url = ?6, expires_at = ?7, updated_at = ?2 WHERE id = ?1",
        params![id, s.sent_at, s.channel.as_str(), s.uploader, s.remote_url, s.short_url, s.expires_at],
    )?;
    Ok(())
}
