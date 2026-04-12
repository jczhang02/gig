use crate::cli::ArchiveArgs;
use crate::ui;
use gig_core::config::{Config, Paths};
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::services::lifecycle::archive_order;
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: ArchiveArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    let order = find_by_id_or_slug(conn, &args.id)?;
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let archive_root = config.general.archive_root;
    let updated = archive_order(conn, order.id, &archive_root, now)?;
    println!("order #{} archived", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
