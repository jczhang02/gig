use crate::cli::NoteArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::services::lifecycle::append_notes;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: NoteArgs) -> Result<()> {
    // Two positionals: `gig note <id> <text>` or one: `gig note <text>` (context for id)
    let (id_opt, text) = match args.text_if_id {
        Some(ref t) => (Some(args.id_or_text.clone()), t.as_str()),
        None => (None, args.id_or_text.as_str()),
    };

    let order = resolve_order(id_opt, conn)?;
    ui::print_banner(&order);
    let updated = append_notes(conn, order.id, text)?;
    println!("updated notes for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
