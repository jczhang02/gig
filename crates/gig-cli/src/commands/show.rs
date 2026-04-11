use crate::cli::ShowArgs;
use crate::ui;
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: ShowArgs) -> Result<()> {
    let order = find_by_id_or_slug(conn, &args.id)?;
    println!("{}", ui::order_detail(&order));
    Ok(())
}
