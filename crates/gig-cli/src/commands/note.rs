use crate::cli::NoteArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::services::lifecycle::append_notes;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: NoteArgs) -> Result<()> {
    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);
    let updated = append_notes(conn, order.id, &args.text)?;
    println!("updated notes for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
