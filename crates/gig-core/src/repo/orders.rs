use crate::models::{Order, OrderStatus, ProjectType};
use crate::{Error, Result};
use rusqlite::{Connection, Row};
use std::str::FromStr;

fn conversion_error(err: Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            err.to_string(),
        )),
    )
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<Order> {
    let status_str: String = row.get("status")?;
    let status = OrderStatus::from_str(&status_str).map_err(conversion_error)?;
    let project_type = row
        .get::<_, Option<String>>("project_type")?
        .map(|value| ProjectType::from_str(&value))
        .transpose()
        .map_err(conversion_error)?;

    Ok(Order {
        id: row.get("id")?,
        slug: row.get("slug")?,
        external_id: row.get("external_id")?,
        title: row.get("title")?,
        client_id: row.get("client_id")?,
        source_org: row.get("source_org")?,
        source_id: row.get("source_id")?,
        project_type,
        status,
        quoted_price: row.get("quoted_price")?,
        final_price: row.get("final_price")?,
        my_cut_ratio: row.get("my_cut_ratio")?,
        currency: row.get("currency")?,
        dev_path: row.get("dev_path")?,
        archive_path: row.get("archive_path")?,
        notes: row.get("notes")?,
        created_at: row.get("created_at")?,
        accepted_at: row.get("accepted_at")?,
        delivered_at: row.get("delivered_at")?,
        paid_at: row.get("paid_at")?,
        archived_at: row.get("archived_at")?,
    })
}

const ALL_COLS: &str = "id, slug, external_id, title, client_id, source_org, source_id, \
     project_type, status, quoted_price, final_price, my_cut_ratio, currency, dev_path, \
     archive_path, notes, created_at, accepted_at, delivered_at, paid_at, archived_at";

/// New order input. Only fields available at creation time.
pub struct NewOrder<'a> {
    pub slug: Option<&'a str>,
    pub external_id: Option<&'a str>,
    pub title: &'a str,
    pub client_id: Option<i64>,
    pub source_org: Option<&'a str>,
    pub source_id: Option<i64>,
    pub project_type: Option<ProjectType>,
    pub status: OrderStatus,
    pub quoted_price: Option<i64>,
    pub final_price: Option<i64>,
    pub my_cut_ratio: f64,
    pub currency: &'a str,
    pub notes: Option<&'a str>,
    pub created_at: i64,
    pub accepted_at: Option<i64>,
}

pub fn insert(conn: &Connection, new: &NewOrder<'_>) -> Result<Order> {
    conn.execute(
        "INSERT INTO orders (slug, external_id, title, client_id, source_org, source_id,
                             project_type, status, quoted_price, final_price, my_cut_ratio,
                             currency, notes, created_at, accepted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        (
            new.slug,
            new.external_id,
            new.title,
            new.client_id,
            new.source_org,
            new.source_id,
            new.project_type.map(|project_type| project_type.as_str()),
            new.status.as_str(),
            new.quoted_price,
            new.final_price,
            new.my_cut_ratio,
            new.currency,
            new.notes,
            new.created_at,
            new.accepted_at,
        ),
    )?;
    find_by_id(conn, conn.last_insert_rowid())
}

pub fn find_by_id(conn: &Connection, id: i64) -> Result<Order> {
    let sql = format!("SELECT {ALL_COLS} FROM orders WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    stmt.query_row((id,), map_row).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::OrderNotFound(id.to_string()),
        other => Error::Db(other),
    })
}

pub fn find_by_slug(conn: &Connection, slug: &str) -> Result<Order> {
    let sql = format!("SELECT {ALL_COLS} FROM orders WHERE slug = ?1");
    let mut stmt = conn.prepare(&sql)?;
    stmt.query_row((slug,), map_row).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::OrderNotFound(slug.to_string()),
        other => Error::Db(other),
    })
}

/// Resolve an identifier that may be a numeric id or a slug.
pub fn find_by_id_or_slug(conn: &Connection, needle: &str) -> Result<Order> {
    if let Ok(id) = needle.parse::<i64>() {
        find_by_id(conn, id)
    } else {
        find_by_slug(conn, needle)
    }
}

pub struct ListFilter {
    pub status: Option<OrderStatus>,
}

pub fn list(conn: &Connection, filter: &ListFilter) -> Result<Vec<Order>> {
    if let Some(status) = filter.status {
        let sql = format!("SELECT {ALL_COLS} FROM orders WHERE status = ?1 ORDER BY id DESC");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map((status.as_str(),), map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        return Ok(rows);
    }

    let sql = format!("SELECT {ALL_COLS} FROM orders ORDER BY id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], map_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Update a single field-tuple used by services. Kept as one internal helper
/// so every partial update goes through the same code path.
pub fn update_status(
    conn: &Connection,
    id: i64,
    status: OrderStatus,
    timestamp_col: Option<&str>,
    timestamp_val: Option<i64>,
) -> Result<()> {
    match (timestamp_col, timestamp_val) {
        (Some(col), Some(val)) => {
            // Safe because `col` is never user-supplied: it's a hard-coded
            // column name from the service layer. We still validate.
            assert!(
                matches!(
                    col,
                    "accepted_at" | "delivered_at" | "paid_at" | "archived_at"
                ),
                "forbidden timestamp col: {col}"
            );
            let sql = format!("UPDATE orders SET status = ?1, {col} = ?2 WHERE id = ?3");
            conn.execute(&sql, (status.as_str(), val, id))?;
        }
        _ => {
            conn.execute(
                "UPDATE orders SET status = ?1 WHERE id = ?2",
                (status.as_str(), id),
            )?;
        }
    }
    Ok(())
}

pub fn update_price(conn: &Connection, id: i64, final_price: Option<i64>) -> Result<()> {
    conn.execute(
        "UPDATE orders SET final_price = ?1 WHERE id = ?2",
        (final_price, id),
    )?;
    Ok(())
}

pub fn update_notes(conn: &Connection, id: i64, notes: Option<&str>) -> Result<()> {
    conn.execute("UPDATE orders SET notes = ?1 WHERE id = ?2", (notes, id))?;
    Ok(())
}

pub fn update_cut_ratio(conn: &Connection, id: i64, ratio: f64) -> Result<()> {
    conn.execute(
        "UPDATE orders SET my_cut_ratio = ?1 WHERE id = ?2",
        (ratio, id),
    )?;
    Ok(())
}

pub fn update_project_type(
    conn: &Connection,
    id: i64,
    project_type: Option<ProjectType>,
) -> Result<()> {
    conn.execute(
        "UPDATE orders SET project_type = ?1 WHERE id = ?2",
        (project_type.map(|value| value.as_str()), id),
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    // CASCADE deletes price_history, requirement_changes, delivery_artifacts, order_tags
    conn.execute("DELETE FROM orders WHERE id = ?1", (id,))?;
    Ok(())
}

pub fn update_dev_path(conn: &Connection, id: i64, dev_path: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE orders SET dev_path = ?1 WHERE id = ?2",
        (dev_path, id),
    )?;
    Ok(())
}

pub fn update_archive_path(conn: &Connection, id: i64, archive_path: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE orders SET archive_path = ?1, dev_path = NULL WHERE id = ?2",
        (archive_path, id),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::ProjectType;

    fn sample(title: &str, status: OrderStatus) -> NewOrder<'_> {
        NewOrder {
            slug: None,
            external_id: None,
            title,
            client_id: None,
            source_org: None,
            source_id: None,
            project_type: None,
            status,
            quoted_price: Some(10_000),
            final_price: None,
            my_cut_ratio: 0.6,
            currency: "CNY",
            notes: None,
            created_at: 1_700_000_000,
            accepted_at: None,
        }
    }

    #[test]
    fn insert_and_find_by_id() {
        let conn = open_in_memory().unwrap();
        let o = insert(&conn, &sample("test", OrderStatus::Lead)).unwrap();
        let again = find_by_id(&conn, o.id).unwrap();
        assert_eq!(o, again);
    }

    #[test]
    fn find_by_id_returns_order_not_found() {
        let conn = open_in_memory().unwrap();
        match find_by_id(&conn, 9999) {
            Err(Error::OrderNotFound(id)) => assert_eq!(id, "9999"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn order_repo_persists_project_type() {
        let conn = open_in_memory().unwrap();
        let mut new = sample("workflow order", OrderStatus::Accepted);
        new.project_type = Some(ProjectType::Crawler);

        let order = insert(&conn, &new).unwrap();
        let again = find_by_id(&conn, order.id).unwrap();

        assert_eq!(again.project_type, Some(ProjectType::Crawler));
    }

    #[test]
    fn update_project_type_backfills_legacy_order() {
        let conn = open_in_memory().unwrap();
        let order = insert(
            &conn,
            &sample("legacy workflow order", OrderStatus::Accepted),
        )
        .unwrap();

        update_project_type(&conn, order.id, Some(ProjectType::Custom)).unwrap();
        let backfilled = find_by_id(&conn, order.id).unwrap();
        assert_eq!(backfilled.project_type, Some(ProjectType::Custom));

        update_project_type(&conn, order.id, None).unwrap();
        let cleared = find_by_id(&conn, order.id).unwrap();
        assert_eq!(cleared.project_type, None);
    }

    #[test]
    fn find_by_id_or_slug_prefers_numeric() {
        let conn = open_in_memory().unwrap();
        let mut n = sample("t", OrderStatus::Lead);
        n.slug = Some("my-slug");
        let o = insert(&conn, &n).unwrap();
        let by_id = find_by_id_or_slug(&conn, &o.id.to_string()).unwrap();
        let by_slug = find_by_id_or_slug(&conn, "my-slug").unwrap();
        assert_eq!(by_id, by_slug);
    }

    #[test]
    fn list_filtered_by_status_returns_only_matches() {
        let conn = open_in_memory().unwrap();
        insert(&conn, &sample("a", OrderStatus::Lead)).unwrap();
        insert(&conn, &sample("b", OrderStatus::Accepted)).unwrap();
        insert(&conn, &sample("c", OrderStatus::Lead)).unwrap();
        let leads = list(
            &conn,
            &ListFilter {
                status: Some(OrderStatus::Lead),
            },
        )
        .unwrap();
        assert_eq!(leads.len(), 2);
        for o in &leads {
            assert_eq!(o.status, OrderStatus::Lead);
        }
    }

    #[test]
    fn list_unfiltered_returns_all_desc() {
        let conn = open_in_memory().unwrap();
        let _a = insert(&conn, &sample("first", OrderStatus::Lead)).unwrap();
        let b = insert(&conn, &sample("second", OrderStatus::Lead)).unwrap();
        let all = list(&conn, &ListFilter { status: None }).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, b.id); // desc order
    }

    #[test]
    fn update_status_with_timestamp_sets_both() {
        let conn = open_in_memory().unwrap();
        let o = insert(&conn, &sample("t", OrderStatus::Accepted)).unwrap();
        update_status(
            &conn,
            o.id,
            OrderStatus::Delivered,
            Some("delivered_at"),
            Some(42),
        )
        .unwrap();
        let again = find_by_id(&conn, o.id).unwrap();
        assert_eq!(again.status, OrderStatus::Delivered);
        assert_eq!(again.delivered_at, Some(42));
    }
}
