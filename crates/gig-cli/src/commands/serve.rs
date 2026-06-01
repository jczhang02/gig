use crate::cli::ServeArgs;
use crate::commands::resolve_order;
use gig_core::repo::order_workflow;
use gig_core::{Error, Result};
use rusqlite::Connection;
use std::path::PathBuf;

pub fn run(conn: &Connection, args: ServeArgs) -> Result<()> {
    let order = resolve_order(Some(args.id_or_slug), conn)?;
    let workflow = order_workflow::find_by_order_id(conn, order.id)?
        .ok_or_else(|| Error::Invalid(format!("missing order_workflow for order {}", order.id)))?;

    let root_dir = workflow
        .gig_dir
        .map(PathBuf::from)
        .ok_or_else(|| Error::Invalid("missing expected gig dir".to_string()))?;
    let index_path = workflow
        .index_path
        .map(PathBuf::from)
        .unwrap_or_else(|| root_dir.join("INDEX.html"));

    gig_gui::serve_static(gig_gui::StaticServeOptions {
        root_dir,
        index_path,
        port: args.port,
        open: args.open,
    })
}
