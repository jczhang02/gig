use crate::cli::ArchiveArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::config::{Config, Paths};
use gig_core::services::lifecycle::{archive_order, archive_order_purge};
use gig_core::Result;
use owo_colors::OwoColorize;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: ArchiveArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);

    // Check for uncommitted git changes (warning only, shown before confirmation).
    let has_dirty_git = if let Some(ref dev_path) = order.dev_path {
        let dev_dir = std::path::Path::new(dev_path);
        if dev_dir.join(".git").exists() {
            let output = std::process::Command::new("git")
                .args(["status", "--porcelain"])
                .current_dir(dev_dir)
                .output();
            if let Ok(out) = output {
                let changes = String::from_utf8_lossy(&out.stdout);
                if !changes.trim().is_empty() {
                    let change_count = changes.lines().count();
                    eprintln!(
                        "{} dev_path has {} uncommitted change(s)",
                        "⚠".yellow(),
                        change_count
                    );
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    let now = OffsetDateTime::now_utc().unix_timestamp();

    if args.purge {
        let prompt = if has_dirty_git {
            "archive and DELETE all local files (including uncommitted changes)?"
        } else {
            "archive and DELETE all local files?"
        };
        if !args.yes && !ui::confirm(prompt) {
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
            let prompt = if has_dirty_git {
                format!("archive to {} (with uncommitted changes)?", dest.display())
            } else {
                format!("archive to {}?", dest.display())
            };
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
