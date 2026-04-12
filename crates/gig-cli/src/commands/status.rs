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
    // Two positionals: `gig status <id> <status>` or one: `gig status <status>` (context for id)
    let (id_opt, status_str) = match args.status_if_id {
        Some(ref s) => (Some(args.id_or_status.clone()), s.as_str()),
        None => (None, args.id_or_status.as_str()),
    };

    let status = OrderStatus::from_str(status_str).map_err(|_| {
        Error::Invalid(format!(
            "unknown status {:?}; valid: lead, negotiating, accepted, in_progress, delivered, paid, archived, cancelled",
            status_str
        ))
    })?;
    let order = resolve_order(id_opt, conn)?;
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
