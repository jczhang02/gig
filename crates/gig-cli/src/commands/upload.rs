//! `gig upload <file>...` — upload arbitrary files to cloud storage.

use crate::cli::UploadArgs;
use gig_core::config::{Config, Paths};
use gig_core::delivery::s3::S3Uploader;
use gig_core::delivery::{UploadOpts, Uploader};
use gig_core::{Error, Result};
use owo_colors::OwoColorize;

pub fn run(args: UploadArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    let uploader_name = args
        .uploader
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&config.delivery.default_uploader);

    if uploader_name.is_empty() {
        return Err(Error::Config(
            "no uploader configured; set delivery.default_uploader in config.toml or pass --uploader"
                .into(),
        ));
    }

    let uploader = build_uploader(uploader_name, &config)?;

    for file in &args.files {
        if !file.exists() {
            eprintln!("{} {} does not exist, skipping", "⚠".yellow(), file.display());
            continue;
        }
        if !file.is_file() {
            eprintln!("{} {} is not a file, skipping", "⚠".yellow(), file.display());
            continue;
        }

        let name = file.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
        eprint!("uploading {}...", name);

        match uploader.upload(file, &UploadOpts { link_ttl_days: None }) {
            Ok(result) => {
                eprintln!(" done ({})", format_file_size(result.file_size));
                println!("{} {}", "→".green().bold(), result.url);

                // Try to copy to clipboard
                if let Ok(mut clip) = arboard::Clipboard::new() {
                    let _ = clip.set_text(&result.url);
                }
            }
            Err(e) => {
                eprintln!(" failed");
                eprintln!("{} {}: {e}", "⚠".yellow(), name);
            }
        }
    }

    Ok(())
}

fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

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
