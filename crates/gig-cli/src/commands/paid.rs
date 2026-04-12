use crate::cli::PaidArgs;
use crate::ui;
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::services::lifecycle::mark_paid;
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: PaidArgs) -> Result<()> {
    let order = find_by_id_or_slug(conn, &args.id)?;
    let paid_at = args
        .on
        .unwrap_or_else(|| OffsetDateTime::now_utc().unix_timestamp());
    let updated = mark_paid(conn, order.id, paid_at)?;
    println!("order #{} marked as paid", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
