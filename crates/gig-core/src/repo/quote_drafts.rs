use crate::models::{ProjectType, QuoteDraft, QuoteDraftStatus};
use crate::Result;
use rusqlite::types::Type;
use rusqlite::{Connection, Row};
use std::str::FromStr;

const ALL_COLS: &str = "id, slug, title, client_label, source_org, project_type, status, summary, \
     quote_min, quote_recommended, quote_max, currency, xdg_path, drop_reason, \
     promoted_order_id, created_at, updated_at, quoted_at, sent_at, accepted_at, dropped_at";

pub struct NewQuoteDraft<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub client_label: Option<&'a str>,
    pub source_org: Option<&'a str>,
    pub project_type: ProjectType,
    pub status: QuoteDraftStatus,
    pub summary: &'a str,
    pub quote_min: Option<i64>,
    pub quote_recommended: Option<i64>,
    pub quote_max: Option<i64>,
    pub currency: &'a str,
    pub xdg_path: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

fn conversion_error(err: crate::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(err))
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<QuoteDraft> {
    let project_type: String = row.get("project_type")?;
    let status: String = row.get("status")?;
    Ok(QuoteDraft {
        id: row.get("id")?,
        slug: row.get("slug")?,
        title: row.get("title")?,
        client_label: row.get("client_label")?,
        source_org: row.get("source_org")?,
        project_type: ProjectType::from_str(&project_type).map_err(conversion_error)?,
        status: QuoteDraftStatus::from_str(&status).map_err(conversion_error)?,
        summary: row.get("summary")?,
        quote_min: row.get("quote_min")?,
        quote_recommended: row.get("quote_recommended")?,
        quote_max: row.get("quote_max")?,
        currency: row.get("currency")?,
        xdg_path: row.get("xdg_path")?,
        drop_reason: row.get("drop_reason")?,
        promoted_order_id: row.get("promoted_order_id")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        quoted_at: row.get("quoted_at")?,
        sent_at: row.get("sent_at")?,
        accepted_at: row.get("accepted_at")?,
        dropped_at: row.get("dropped_at")?,
    })
}

pub fn insert(conn: &Connection, new: &NewQuoteDraft<'_>) -> Result<QuoteDraft> {
    conn.execute(
        "INSERT INTO quote_drafts \
         (slug, title, client_label, source_org, project_type, status, summary, quote_min, \
          quote_recommended, quote_max, currency, xdg_path, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        (
            new.slug,
            new.title,
            new.client_label,
            new.source_org,
            new.project_type.as_str(),
            new.status.as_str(),
            new.summary,
            new.quote_min,
            new.quote_recommended,
            new.quote_max,
            new.currency,
            new.xdg_path,
            new.created_at,
            new.updated_at,
        ),
    )?;
    find_by_id(conn, conn.last_insert_rowid())
}

pub fn find_by_id(conn: &Connection, id: i64) -> Result<QuoteDraft> {
    let sql = format!("SELECT {ALL_COLS} FROM quote_drafts WHERE id = ?1");
    Ok(conn.query_row(&sql, (id,), map_row)?)
}

pub fn find_by_slug(conn: &Connection, slug: &str) -> Result<Option<QuoteDraft>> {
    let sql = format!("SELECT {ALL_COLS} FROM quote_drafts WHERE slug = ?1 LIMIT 1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map((slug,), map_row)?;
    if let Some(row) = rows.next() {
        Ok(Some(row?))
    } else {
        Ok(None)
    }
}

pub fn list(conn: &Connection) -> Result<Vec<QuoteDraft>> {
    let sql = format!("SELECT {ALL_COLS} FROM quote_drafts ORDER BY id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], map_row)?;
    rows.map(|r| r.map_err(Into::into)).collect()
}

pub fn update_pricing(
    conn: &Connection,
    id: i64,
    quote_min: i64,
    quote_recommended: i64,
    quote_max: i64,
    updated_at: &str,
) -> Result<QuoteDraft> {
    conn.execute(
        "UPDATE quote_drafts \
         SET status = 'quoted', quote_min = ?2, quote_recommended = ?3, quote_max = ?4, \
             quoted_at = ?5, updated_at = ?5 \
         WHERE id = ?1",
        (id, quote_min, quote_recommended, quote_max, updated_at),
    )?;
    find_by_id(conn, id)
}

pub fn mark_sent(conn: &Connection, id: i64, updated_at: &str) -> Result<QuoteDraft> {
    conn.execute(
        "UPDATE quote_drafts SET sent_at = ?2, updated_at = ?2 WHERE id = ?1",
        (id, updated_at),
    )?;
    find_by_id(conn, id)
}

pub fn mark_accepted(
    conn: &Connection,
    id: i64,
    promoted_order_id: i64,
    accepted_at: &str,
) -> Result<QuoteDraft> {
    conn.execute(
        "UPDATE quote_drafts          SET status = 'accepted', promoted_order_id = ?2, accepted_at = ?3, updated_at = ?3          WHERE id = ?1",
        (id, promoted_order_id, accepted_at),
    )?;
    find_by_id(conn, id)
}

pub fn mark_dropped(
    conn: &Connection,
    id: i64,
    drop_reason: &str,
    updated_at: &str,
) -> Result<QuoteDraft> {
    conn.execute(
        "UPDATE quote_drafts \
         SET status = 'dropped', drop_reason = ?2, dropped_at = ?3, updated_at = ?3 \
         WHERE id = ?1",
        (id, drop_reason, updated_at),
    )?;
    find_by_id(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::{ProjectType, QuoteDraftStatus};

    fn sample<'a>() -> NewQuoteDraft<'a> {
        NewQuoteDraft {
            slug: "draft-1",
            title: "Build a crawler",
            client_label: Some("Client A"),
            source_org: Some("wechat"),
            project_type: ProjectType::Crawler,
            status: QuoteDraftStatus::QuoteDraft,
            summary: "Need a crawler for listings",
            quote_min: None,
            quote_recommended: None,
            quote_max: None,
            currency: "CNY",
            xdg_path: "/tmp/gig/quotes/draft-1",
            created_at: "2026-05-27T00:00:00Z",
            updated_at: "2026-05-27T00:00:00Z",
        }
    }

    #[test]
    fn insert_find_and_list_quote_drafts() {
        let conn = open_in_memory().unwrap();
        let draft = insert(&conn, &sample()).unwrap();

        assert_eq!(draft.slug, "draft-1");
        assert_eq!(draft.project_type, ProjectType::Crawler);
        assert_eq!(draft.status, QuoteDraftStatus::QuoteDraft);

        let by_slug = find_by_slug(&conn, "draft-1").unwrap().unwrap();
        assert_eq!(by_slug, draft);

        let listed = list(&conn).unwrap();
        assert_eq!(listed, vec![draft]);
    }

    #[test]
    fn update_pricing_and_drop_quote_draft() {
        let conn = open_in_memory().unwrap();
        let draft = insert(&conn, &sample()).unwrap();

        let priced = update_pricing(
            &conn,
            draft.id,
            10_000,
            15_000,
            20_000,
            "2026-05-27T01:00:00Z",
        )
        .unwrap();
        assert_eq!(priced.status, QuoteDraftStatus::Quoted);
        assert_eq!(priced.quote_min, Some(10_000));
        assert_eq!(priced.quote_recommended, Some(15_000));
        assert_eq!(priced.quote_max, Some(20_000));
        assert_eq!(priced.quoted_at.as_deref(), Some("2026-05-27T01:00:00Z"));

        let dropped =
            mark_dropped(&conn, draft.id, "client paused", "2026-05-27T02:00:00Z").unwrap();
        assert_eq!(dropped.status, QuoteDraftStatus::Dropped);
        assert_eq!(dropped.drop_reason.as_deref(), Some("client paused"));
        assert_eq!(dropped.dropped_at.as_deref(), Some("2026-05-27T02:00:00Z"));
    }
}
