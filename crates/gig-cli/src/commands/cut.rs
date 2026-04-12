use crate::cli::CutArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::services::lifecycle::update_cut_ratio;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: CutArgs) -> Result<()> {
    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);
    let updated = update_cut_ratio(conn, order.id, args.ratio)?;
    println!("updated cut ratio for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
