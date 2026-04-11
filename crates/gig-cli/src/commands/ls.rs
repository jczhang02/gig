use crate::cli::LsArgs;
use crate::ui;
use gig_core::models::OrderStatus;
use gig_core::repo::orders::{list, ListFilter};
use gig_core::{Error, Result};
use rusqlite::Connection;
use std::str::FromStr;

pub fn run(conn: &Connection, args: LsArgs) -> Result<()> {
    let status = match args.status.as_deref() {
        Some(s) => Some(OrderStatus::from_str(s).map_err(|_| {
            Error::Invalid(format!(
                "unknown status {s:?}; valid: lead, negotiating, accepted, in_progress, delivered, paid, archived, cancelled"
            ))
        })?),
        None => None,
    };
    let filter = ListFilter { status };
    let orders = list(conn, &filter)?;
    if orders.is_empty() {
        println!("no orders");
        return Ok(());
    }
    let table = ui::orders_table(&orders);
    println!("{table}");
    Ok(())
}
