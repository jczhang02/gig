use crate::cli::StatusArgs;
use crate::ui;
use gig_core::models::OrderStatus;
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::services::lifecycle::set_status;
use gig_core::{Error, Result};
use rusqlite::Connection;
use std::str::FromStr;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: StatusArgs) -> Result<()> {
    let status = OrderStatus::from_str(&args.status).map_err(|_| {
        Error::Invalid(format!(
            "unknown status {:?}; valid: lead, negotiating, accepted, in_progress, delivered, paid, archived, cancelled",
            args.status
        ))
    })?;
    let order = find_by_id_or_slug(conn, &args.id)?;
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let updated = set_status(conn, order.id, status, now)?;
    println!("status updated for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
