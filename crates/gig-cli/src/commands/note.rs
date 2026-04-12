use crate::cli::NoteArgs;
use crate::ui;
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::services::lifecycle::append_notes;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: NoteArgs) -> Result<()> {
    let order = find_by_id_or_slug(conn, &args.id)?;
    let updated = append_notes(conn, order.id, &args.text)?;
    println!("updated notes for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
