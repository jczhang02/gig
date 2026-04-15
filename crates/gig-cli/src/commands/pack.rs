//! `gig pack <id>` — collect files and create an archive.

use crate::cli::PackArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::config::{Config, Paths};
use gig_core::services::pack::{pack_order, PackFormat};
use gig_core::Result;
use rusqlite::Connection;
use std::path::PathBuf;

pub fn run(conn: &Connection, args: PackArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);

    let dev_path = order.dev_path.as_deref().ok_or_else(|| {
        gig_core::Error::Invalid(format!(
            "order #{} has no dev_path; run `gig init {}` first",
            order.id, order.id
        ))
    })?;
    let project_dir = PathBuf::from(dev_path);

    // Determine format: CLI flag > config default.
    let fmt_str = if args.format != "zip" {
        args.format.clone()
    } else {
        config.pack.default_format.clone()
    };
    let format = PackFormat::parse(&fmt_str).unwrap_or(PackFormat::Zip);

    // Determine output path.
    let output = match args.output {
        Some(p) => p,
        None => {
            let id_str = order.id.to_string();
            let slug = order.slug.as_deref().unwrap_or(&id_str);
            let tmp = std::env::temp_dir();
            tmp.join(format!("gig-{}-{}.{}", order.id, slug, format.extension()))
        }
    };

    let result = pack_order(
        &project_dir,
        &output,
        &format,
        &config.pack.extra_ignore,
        args.dry_run,
    )?;

    if args.dry_run {
        eprintln!(
            "dry-run: {} file(s) would be packed into {}",
            result.files_count,
            result.archive_path.display()
        );
    } else {
        let size = std::fs::metadata(&result.archive_path)
            .map(|m| m.len())
            .unwrap_or(0);
        println!(
            "packed {} file(s)  →  {}  ({})",
            result.files_count,
            result.archive_path.display(),
            human_size(size),
        );
    }

    Ok(())
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
