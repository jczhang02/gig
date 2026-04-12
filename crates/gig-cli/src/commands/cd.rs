use crate::cli::CdArgs;
use crate::commands::resolve_order;
use gig_core::{Error, Result};
use rusqlite::Connection;

pub fn run(conn: &Connection, args: CdArgs) -> Result<()> {
    let order = resolve_order(args.id, conn)?;
    let dev_path = order
        .dev_path
        .as_deref()
        .ok_or_else(|| Error::Invalid("order has no dev_path (not yet initialized)".into()))?;
    print!("{dev_path}");
    Ok(())
}
