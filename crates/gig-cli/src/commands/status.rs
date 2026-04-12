use crate::cli::StatusArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::models::OrderStatus;
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
    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);

    // Destructive status changes require confirmation unless --yes
    if !args.yes {
        let prompt = format!(
            "set status {} \u{2192} {}?",
            order.status.as_str(),
            status.as_str()
        );
        if !ui::confirm(&prompt) {
            eprintln!("aborted.");
            return Ok(());
        }
    }

    let now = OffsetDateTime::now_utc().unix_timestamp();
    let updated = set_status(conn, order.id, status, now)?;
    println!("status updated for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
