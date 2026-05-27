use crate::cli::PaidArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::services::lifecycle::mark_paid;
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: PaidArgs) -> Result<()> {
    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);
    let paid_at = args
        .on
        .unwrap_or_else(|| OffsetDateTime::now_utc().unix_timestamp());
    let was_archived = order.status == gig_core::models::OrderStatus::Archived;
    let updated = mark_paid(conn, order.id, paid_at)?;
    if was_archived {
        println!(
            "order #{} payment recorded (status remains archived)",
            updated.id
        );
    } else {
        println!("order #{} marked as paid", updated.id);
    }
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
