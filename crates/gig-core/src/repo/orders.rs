use crate::models::{Order, OrderStatus, ProjectType};
use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};

const COLS: &str = "id, slug, title, material_path, platform, external_id, project_type, status, currency, price_minor, cut_ratio, dev_path, archive_path, client_words, notes, created_at, started_at, delivered_at, paid_at, warranty_until, archived_at, cancelled_at, cancel_reason, legacy_id";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Order> {
    let project_type: String = r.get(6)?;
    let status: String = r.get(7)?;
    Ok(Order {
        id: r.get(0)?,
        slug: r.get(1)?,
        title: r.get(2)?,
        material_path: r.get(3)?,
        platform: r.get(4)?,
        external_id: r.get(5)?,
        project_type: ProjectType::parse(&project_type).map_err(bad_col)?,
        status: OrderStatus::parse(&status).map_err(bad_col)?,
        currency: r.get(8)?,
        price_minor: r.get(9)?,
        price: None,
        cut_ratio: r.get(10)?,
        dev_path: r.get(11)?,
        archive_path: r.get(12)?,
        client_words: r.get(13)?,
        notes: r.get(14)?,
        created_at: r.get(15)?,
        started_at: r.get(16)?,
        delivered_at: r.get(17)?,
        paid_at: r.get(18)?,
        warranty_until: r.get(19)?,
        archived_at: r.get(20)?,
        cancelled_at: r.get(21)?,
        cancel_reason: r.get(22)?,
        legacy_id: r.get(23)?,
    }
    .with_price())
}

fn bad_col(e: Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

pub struct NewOrder<'a> {
    pub id: Option<i64>,
    pub slug: &'a str,
    pub title: &'a str,
    pub material_path: Option<&'a str>,
    pub platform: Option<&'a str>,
    pub external_id: Option<&'a str>,
    pub project_type: ProjectType,
    pub status: OrderStatus,
    pub currency: &'a str,
    pub price_minor: Option<i64>,
    pub cut_ratio: f64,
    pub dev_path: Option<&'a str>,
    pub archive_path: Option<&'a str>,
    pub client_words: Option<&'a str>,
    pub notes: &'a str,
    pub created_at: &'a str,
    pub started_at: Option<&'a str>,
    pub delivered_at: Option<&'a str>,
    pub paid_at: Option<&'a str>,
    pub warranty_until: Option<&'a str>,
    pub archived_at: Option<&'a str>,
    pub cancelled_at: Option<&'a str>,
    pub cancel_reason: Option<&'a str>,
    pub legacy_id: Option<i64>,
}

pub fn insert(conn: &Connection, n: &NewOrder<'_>) -> Result<Order> {
    conn.execute(
        "INSERT INTO orders (id, slug, title, material_path, platform, external_id, project_type, status, currency, price_minor, cut_ratio, dev_path, archive_path, client_words, notes, created_at, started_at, delivered_at, paid_at, warranty_until, archived_at, cancelled_at, cancel_reason, legacy_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        params![
            n.id, n.slug, n.title, n.material_path, n.platform, n.external_id,
            n.project_type.as_str(), n.status.as_str(), n.currency, n.price_minor, n.cut_ratio,
            n.dev_path, n.archive_path, n.client_words, n.notes, n.created_at, n.started_at,
            n.delivered_at, n.paid_at, n.warranty_until, n.archived_at, n.cancelled_at,
            n.cancel_reason, n.legacy_id
        ],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            Error::InvalidInput(format!("order slug {} already exists", n.slug))
        }
        other => Error::Db(other),
    })?;
    find_by_id(conn, conn.last_insert_rowid())
}

pub fn find_by_id(conn: &Connection, id: i64) -> Result<Order> {
    conn.query_row(
        &format!("SELECT {COLS} FROM orders WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or_else(|| Error::NotFound(format!("order #{id}")))
}

pub fn find_by_slug(conn: &Connection, slug: &str) -> Result<Option<Order>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM orders WHERE slug = ?1"),
            [slug],
            from_row,
        )
        .optional()?)
}

/// `#12`, `12` or a slug.
pub fn resolve(conn: &Connection, key: &str) -> Result<Order> {
    let k = key.trim();
    if let Ok(id) = k.trim_start_matches('#').parse::<i64>() {
        if let Some(o) = conn
            .query_row(
                &format!("SELECT {COLS} FROM orders WHERE id = ?1"),
                [id],
                from_row,
            )
            .optional()?
        {
            return Ok(o);
        }
    }
    find_by_slug(conn, k)?.ok_or_else(|| Error::NotFound(format!("order {key:?}")))
}

pub fn list(conn: &Connection, include_closed: bool) -> Result<Vec<Order>> {
    let sql = if include_closed {
        format!("SELECT {COLS} FROM orders ORDER BY id DESC")
    } else {
        format!("SELECT {COLS} FROM orders WHERE status NOT IN ('archived','cancelled') ORDER BY id DESC")
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn set_status(conn: &Connection, id: i64, status: OrderStatus) -> Result<()> {
    conn.execute(
        "UPDATE orders SET status = ?2 WHERE id = ?1",
        params![id, status.as_str()],
    )?;
    Ok(())
}

/// Set one text column. `col` is a compile-time constant at every call site.
pub fn set_text(conn: &Connection, id: i64, col: &'static str, value: Option<&str>) -> Result<()> {
    conn.execute(
        &format!("UPDATE orders SET {col} = ?2 WHERE id = ?1"),
        params![id, value],
    )?;
    Ok(())
}

pub fn set_price(conn: &Connection, id: i64, price_minor: Option<i64>) -> Result<()> {
    conn.execute(
        "UPDATE orders SET price_minor = ?2 WHERE id = ?1",
        params![id, price_minor],
    )?;
    Ok(())
}

pub fn append_note(conn: &Connection, id: i64, at: &str, text: &str) -> Result<()> {
    let line = format!("[{at}] {}\n", text.trim());
    conn.execute(
        "UPDATE orders SET notes = notes || ?2 WHERE id = ?1",
        params![id, line],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM orders WHERE id = ?1", [id])?;
    Ok(())
}

/// (id, dev_path, archive_path) for every order, for cwd resolution and doctor.
pub type OrderPaths = (i64, Option<String>, Option<String>);

pub fn paths(conn: &Connection) -> Result<Vec<OrderPaths>> {
    let mut stmt = conn.prepare("SELECT id, dev_path, archive_path FROM orders")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
