use crate::cli::ChangeArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::services::lifecycle::add_requirement_change;
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: ChangeArgs) -> Result<()> {
    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let updated = add_requirement_change(conn, order.id, &args.message, args.delta, now)?;
    println!("recorded requirement change for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
