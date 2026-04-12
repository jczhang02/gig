use crate::cli::ArchiveArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::config::{Config, Paths};
use gig_core::services::lifecycle::archive_order;
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: ArchiveArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);

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

    let now = OffsetDateTime::now_utc().unix_timestamp();
    let updated = archive_order(conn, order.id, archive_root, now)?;
    println!("order #{} archived", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
