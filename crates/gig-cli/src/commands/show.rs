use crate::cli::ShowArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::repo::sources;
use gig_core::services::lifecycle::{get_price_history, get_requirement_changes, list_tags};
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: ShowArgs) -> Result<()> {
    let order = resolve_order(args.id, conn)?;
    let tags = list_tags(conn, order.id)?;
    let price_history = get_price_history(conn, order.id)?;
    let req_changes = get_requirement_changes(conn, order.id)?;

    // Look up source entity if present
    let source_label: Option<String> = match order.source_id {
        Some(sid) => match sources::find_by_id(conn, sid) {
            Ok(s) => Some(format!("{} (cut {:.0}%)", s.name, s.cut_ratio * 100.0)),
            Err(_) => None,
        },
        None => None,
    };

    println!(
        "{}",
        ui::order_detail_full(
            &order,
            &tags,
            &price_history,
            &req_changes,
            source_label.as_deref()
        )
    );
    Ok(())
}
