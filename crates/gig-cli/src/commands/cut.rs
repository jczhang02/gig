use crate::cli::CutArgs;
use crate::ui;
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::services::lifecycle::update_cut_ratio;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: CutArgs) -> Result<()> {
    let order = find_by_id_or_slug(conn, &args.id)?;
    let updated = update_cut_ratio(conn, order.id, args.ratio)?;
    println!("updated cut ratio for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
