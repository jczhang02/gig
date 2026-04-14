use crate::cli::ArchiveArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::config::{Config, Paths};
use gig_core::services::lifecycle::{archive_order, archive_order_purge};
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: ArchiveArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);

    let now = OffsetDateTime::now_utc().unix_timestamp();

    if args.purge {
        if !args.yes && !ui::confirm("archive and DELETE all local files?") {
            eprintln!("aborted.");
            return Ok(());
        }
        let updated = archive_order_purge(conn, order.id, now)?;
        println!("order #{} archived (local files purged)", updated.id);
        println!("{}", ui::order_detail(&updated));
    } else {
        let archive_root = &config.general.archive_root;
        if !args.yes {
            let dest = std::path::Path::new(archive_root)
                .join(order.slug.as_deref().unwrap_or(&order.id.to_string()));
            let prompt = format!("archive to {}?", dest.display());
            if !ui::confirm(&prompt) {
                eprintln!("aborted.");
                return Ok(());
            }
        }
        let updated = archive_order(conn, order.id, archive_root, now)?;
        println!("order #{} archived", updated.id);
        println!("{}", ui::order_detail(&updated));
    }

    Ok(())
}
