use crate::models::{Draft, DraftStatus, ProjectType};
use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};

const COLS: &str = "id, slug, title, material_path, project_type, notes_dir, status, drop_reason, notes_snapshot, promoted_order_id, created_at, closed_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Draft> {
    let project_type: Option<String> = r.get(4)?;
    let status: String = r.get(6)?;
    let conv = |e: Error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    };
    Ok(Draft {
        id: r.get(0)?,
        slug: r.get(1)?,
        title: r.get(2)?,
        material_path: r.get(3)?,
        project_type: project_type
            .map(|t| ProjectType::parse(&t))
            .transpose()
            .map_err(conv)?,
        notes_dir: r.get(5)?,
        status: DraftStatus::parse(&status).map_err(conv)?,
        drop_reason: r.get(7)?,
        notes_snapshot: r.get(8)?,
        promoted_order_id: r.get(9)?,
        created_at: r.get(10)?,
        closed_at: r.get(11)?,
    })
}

pub struct NewDraft<'a> {
    pub slug: &'a str,
    pub title: Option<&'a str>,
    pub material_path: Option<&'a str>,
    pub project_type: Option<ProjectType>,
    pub notes_dir: &'a str,
    pub status: DraftStatus,
    pub drop_reason: Option<&'a str>,
    pub notes_snapshot: Option<&'a str>,
    pub promoted_order_id: Option<i64>,
    pub created_at: &'a str,
    pub closed_at: Option<&'a str>,
}

pub fn insert(conn: &Connection, n: &NewDraft<'_>) -> Result<Draft> {
    conn.execute(
        "INSERT INTO drafts (slug, title, material_path, project_type, notes_dir, status, drop_reason, notes_snapshot, promoted_order_id, created_at, closed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            n.slug, n.title, n.material_path, n.project_type.map(|t| t.as_str()), n.notes_dir,
            n.status.as_str(), n.drop_reason, n.notes_snapshot, n.promoted_order_id, n.created_at,
            n.closed_at
        ],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            Error::InvalidInput(format!("draft slug {} already exists", n.slug))
        }
        other => Error::Db(other),
    })?;
    find_by_id(conn, conn.last_insert_rowid())
}

pub fn find_by_id(conn: &Connection, id: i64) -> Result<Draft> {
    conn.query_row(
        &format!("SELECT {COLS} FROM drafts WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or_else(|| Error::NotFound(format!("draft #{id}")))
}

pub fn find_by_slug(conn: &Connection, slug: &str) -> Result<Option<Draft>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM drafts WHERE slug = ?1"),
            [slug],
            from_row,
        )
        .optional()?)
}

pub fn list(conn: &Connection, all: bool) -> Result<Vec<Draft>> {
    let sql = if all {
        format!("SELECT {COLS} FROM drafts ORDER BY id DESC")
    } else {
        format!("SELECT {COLS} FROM drafts WHERE status = 'open' ORDER BY id DESC")
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn close(
    conn: &Connection,
    id: i64,
    status: DraftStatus,
    drop_reason: Option<&str>,
    notes_snapshot: Option<&str>,
    promoted_order_id: Option<i64>,
    at: &str,
) -> Result<Draft> {
    conn.execute(
        "UPDATE drafts SET status = ?2, drop_reason = ?3, notes_snapshot = ?4, promoted_order_id = ?5, closed_at = ?6 WHERE id = ?1",
        params![id, status.as_str(), drop_reason, notes_snapshot, promoted_order_id, at],
    )?;
    find_by_id(conn, id)
}
