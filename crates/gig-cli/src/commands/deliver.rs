//! `gig deliver <id>` — pack, upload, record artifact, transition to delivered.

use crate::cli::DeliverArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::config::{Config, Paths};
use gig_core::delivery::s3::S3Uploader;
use gig_core::delivery::{UploadOpts, Uploader};
use gig_core::models::OrderStatus;
use gig_core::repo::delivery_artifacts;
use gig_core::services::lifecycle::set_status;
use gig_core::services::pack::{pack_order, PackFormat};
use gig_core::{Error, Result};
use rusqlite::Connection;
use std::path::PathBuf;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: DeliverArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);

    // Determine uploader config (CLI flag > config default).
    let uploader_name = args
        .uploader
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(config.delivery.default_uploader.as_str());

    if uploader_name.is_empty() {
        return Err(Error::Config(
            "no uploader configured; set [delivery] default_uploader in config.toml \
             or pass --uploader"
                .into(),
        ));
    }

    // Determine format: CLI flag > config default > "zip".
    let fmt_str = args
        .format
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&config.pack.default_format);
    let format = PackFormat::parse(fmt_str).unwrap_or(PackFormat::Zip);

    // --dry-run: pack with dry_run=true, print what would be uploaded, then stop.
    if args.dry_run {
        let dev_path = order.dev_path.as_deref().ok_or_else(|| {
            Error::Invalid(format!(
                "order #{} has no dev_path; run `gig init {}` first",
                order.id, order.id
            ))
        })?;
        let project_dir = PathBuf::from(dev_path);
        let id_str = order.id.to_string();
        let slug = order.slug.as_deref().unwrap_or(&id_str);
        let tmp = std::env::temp_dir();
        let output = tmp.join(format!("gig-{}.{}", slug, format.extension()));

        println!("(dry-run) would pack: {}", project_dir.display());
        println!("(dry-run) would write: {}", output.display());
        println!("(dry-run) would upload to: {uploader_name}");
        let result = pack_order(
            &project_dir,
            &output,
            &format,
            &config.pack.extra_ignore,
            true,
        )?;
        println!(
            "(dry-run) files that would be included: {}",
            result.files_count
        );
        return Ok(());
    }

    // Resolve archive path.
    let archive_path = if args.resend {
        // --resend: use most-recent artifact's local_path, or error.
        let artifact = delivery_artifacts::latest_for_order(conn, order.id)?.ok_or_else(|| {
            Error::Invalid(format!(
                "no previous delivery artifact for order #{} — cannot --resend",
                order.id
            ))
        })?;
        let local = artifact
            .local_path
            .ok_or_else(|| Error::Invalid("previous artifact has no local_path stored".into()))?;
        let p = PathBuf::from(&local);
        if !p.exists() {
            return Err(Error::Invalid(format!(
                "previous archive not found on disk: {local}"
            )));
        }
        p
    } else {
        // Normal path: pack the project.
        let dev_path = order.dev_path.as_deref().ok_or_else(|| {
            Error::Invalid(format!(
                "order #{} has no dev_path; run `gig init {}` first",
                order.id, order.id
            ))
        })?;
        let project_dir = PathBuf::from(dev_path);
        let id_str = order.id.to_string();
        let slug = order.slug.as_deref().unwrap_or(&id_str);
        let tmp = std::env::temp_dir();
        let output = tmp.join(format!("gig-{}.{}", slug, format.extension()));

        eprint!("packing...");
        let result = pack_order(
            &project_dir,
            &output,
            &format,
            &config.pack.extra_ignore,
            false,
        )?;
        eprintln!(" {} file(s)", result.files_count);
        result.archive_path
    };

    let uploader: Box<dyn Uploader> = build_uploader(uploader_name, &config)?;

    eprint!("uploading to {}...", uploader_name);
    let upload_result = uploader.upload(
        &archive_path,
        &UploadOpts {
            link_ttl_days: None,
        },
    )?;
    eprintln!(" done");

    let now = OffsetDateTime::now_utc().unix_timestamp();

    // Record delivery artifact.
    delivery_artifacts::insert(
        conn,
        order.id,
        Some(&archive_path.to_string_lossy()),
        Some(uploader_name),
        Some(&upload_result.url),
        upload_result.expires_at,
        now,
    )?;

    // Transition status → delivered.
    let updated = set_status(conn, order.id, OrderStatus::Delivered, now)?;

    println!("link: {}", upload_result.url);
    println!("status: {} → {}", order.status, updated.status);

    // Attempt clipboard copy. Gracefully degrade if no display server.
    match copy_to_clipboard(&upload_result.url) {
        Ok(()) => eprintln!("(URL copied to clipboard)"),
        Err(e) => eprintln!("(clipboard unavailable: {e})"),
    }

    Ok(())
}

/// Build an `Uploader` from a config name string and loaded config.
///
/// Supported formats:
/// - `"s3:<name>"` → look up `[delivery.s3.<name>]` and construct `S3Uploader`
fn build_uploader(name: &str, config: &Config) -> Result<Box<dyn Uploader>> {
    let s3_name = name.strip_prefix("s3:").ok_or_else(|| {
        Error::Config(format!(
            "unsupported uploader '{name}'; use 's3:<name>' format"
        ))
    })?;
    let s3_cfg = config.delivery.s3.get(s3_name).ok_or_else(|| {
        Error::Config(format!(
            "no [delivery.s3.{s3_name}] section found in config.toml"
        ))
    })?;
    Ok(Box::new(S3Uploader::new(name.to_string(), s3_cfg)?))
}

fn copy_to_clipboard(text: &str) -> std::result::Result<(), String> {
    let mut ctx = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    ctx.set_text(text).map_err(|e| e.to_string())
}
