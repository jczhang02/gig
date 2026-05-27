use crate::models::{DeliveryPackage, DeliveryPackageStatus};
use crate::Result;
use rusqlite::types::Type;
use rusqlite::{Connection, Row};
use std::str::FromStr;

const ALL_COLS: &str = "id, order_id, delivery_date, delivery_dir, client_dir, manifest_path, \
     package_path, status, created_at, updated_at";

pub struct NewDeliveryPackage<'a> {
    pub order_id: i64,
    pub delivery_date: &'a str,
    pub delivery_dir: &'a str,
    pub client_dir: &'a str,
    pub manifest_path: &'a str,
    pub package_path: Option<&'a str>,
    pub status: DeliveryPackageStatus,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

fn conversion_error(err: crate::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(err))
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<DeliveryPackage> {
    let status: String = row.get("status")?;
    Ok(DeliveryPackage {
        id: row.get("id")?,
        order_id: row.get("order_id")?,
        delivery_date: row.get("delivery_date")?,
        delivery_dir: row.get("delivery_dir")?,
        client_dir: row.get("client_dir")?,
        manifest_path: row.get("manifest_path")?,
        package_path: row.get("package_path")?,
        status: DeliveryPackageStatus::from_str(&status).map_err(conversion_error)?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn insert(conn: &Connection, new: &NewDeliveryPackage<'_>) -> Result<DeliveryPackage> {
    conn.execute(
        "INSERT INTO delivery_packages \
         (order_id, delivery_date, delivery_dir, client_dir, manifest_path, package_path, \
          status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        (
            new.order_id,
            new.delivery_date,
            new.delivery_dir,
            new.client_dir,
            new.manifest_path,
            new.package_path,
            new.status.as_str(),
            new.created_at,
            new.updated_at,
        ),
    )?;
    find_by_id(conn, conn.last_insert_rowid())
}

pub fn find_by_id(conn: &Connection, id: i64) -> Result<DeliveryPackage> {
    let sql = format!("SELECT {ALL_COLS} FROM delivery_packages WHERE id = ?1");
    Ok(conn.query_row(&sql, (id,), map_row)?)
}

pub fn list_for_order(conn: &Connection, order_id: i64) -> Result<Vec<DeliveryPackage>> {
    let sql =
        format!("SELECT {ALL_COLS} FROM delivery_packages WHERE order_id = ?1 ORDER BY id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map((order_id,), map_row)?;
    rows.map(|row| row.map_err(Into::into)).collect()
}

pub fn list_all(conn: &Connection) -> Result<Vec<DeliveryPackage>> {
    let sql = format!("SELECT {ALL_COLS} FROM delivery_packages ORDER BY id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], map_row)?;
    rows.map(|row| row.map_err(Into::into)).collect()
}

pub fn update_status(
    conn: &Connection,
    id: i64,
    status: DeliveryPackageStatus,
    updated_at: &str,
) -> Result<DeliveryPackage> {
    conn.execute(
        "UPDATE delivery_packages SET status = ?2, updated_at = ?3 WHERE id = ?1",
        (id, status.as_str(), updated_at),
    )?;
    find_by_id(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::{DeliveryPackageStatus, OrderStatus};
    use crate::repo::orders::{self, NewOrder};

    fn sample_order(conn: &rusqlite::Connection) -> i64 {
        orders::insert(
            conn,
            &NewOrder {
                slug: Some("delivery-order"),
                external_id: None,
                title: "Delivery order",
                client_id: None,
                source_org: None,
                source_id: None,
                project_type: None,
                status: OrderStatus::ReadyToDeliver,
                quoted_price: Some(15_000),
                final_price: None,
                my_cut_ratio: 0.6,
                currency: "CNY",
                notes: None,
                created_at: 1,
                accepted_at: Some(1),
            },
        )
        .unwrap()
        .id
    }

    fn sample<'a>(order_id: i64) -> NewDeliveryPackage<'a> {
        NewDeliveryPackage {
            order_id,
            delivery_date: "2026-05-27",
            delivery_dir: "/work/project/.gig/delivery/2026-05-27",
            client_dir: "/work/project/.gig/delivery/2026-05-27/client",
            manifest_path: "/work/project/.gig/delivery/2026-05-27/manifest.toml",
            package_path: Some("/work/project/.gig/delivery/2026-05-27/export/client-package.zip"),
            status: DeliveryPackageStatus::Prepared,
            created_at: "2026-05-27T00:00:00Z",
            updated_at: "2026-05-27T00:00:00Z",
        }
    }

    #[test]
    fn insert_find_and_list_delivery_packages() {
        let conn = open_in_memory().unwrap();
        let order_id = sample_order(&conn);
        let package = insert(&conn, &sample(order_id)).unwrap();

        assert_eq!(package.order_id, order_id);
        assert_eq!(package.status, DeliveryPackageStatus::Prepared);

        let found = find_by_id(&conn, package.id).unwrap();
        assert_eq!(found, package);

        let listed = list_for_order(&conn, order_id).unwrap();
        assert_eq!(listed, vec![package]);
    }

    #[test]
    fn update_delivery_package_status() {
        let conn = open_in_memory().unwrap();
        let order_id = sample_order(&conn);
        let package = insert(&conn, &sample(order_id)).unwrap();

        let validated = update_status(
            &conn,
            package.id,
            DeliveryPackageStatus::Validated,
            "2026-05-27T04:00:00Z",
        )
        .unwrap();

        assert_eq!(validated.status, DeliveryPackageStatus::Validated);
        assert_eq!(validated.updated_at, "2026-05-27T04:00:00Z");
    }
}
