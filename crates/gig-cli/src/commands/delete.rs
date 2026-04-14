use crate::cli::DeleteArgs;
use crate::ui;
use gig_core::repo::orders::{self, find_by_id_or_slug};
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: DeleteArgs) -> Result<()> {
    let order = find_by_id_or_slug(conn, &args.id)?;
    ui::print_banner(&order);

    if !args.yes && !ui::confirm("delete this order permanently?") {
        eprintln!("aborted.");
        return Ok(());
    }

    orders::delete(conn, order.id)?;
    println!("deleted order #{} ({})", order.id, order.slug.as_deref().unwrap_or(&order.title));
    Ok(())
}
