//! `gig import` command: register existing project directories into the DB.

use crate::cli::ImportArgs;
use gig_core::config::{Config, Paths};
use gig_core::models::OrderStatus;
use gig_core::services::import::{
    import_project, infer_created_at, infer_status, parse_date_prefix, relocate_project, slugify,
    ImportInput, ImportResult,
};
use gig_core::{Error, Result};
use rusqlite::Connection;
use std::io::{self, BufRead, Write as IoWrite};
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub fn run(conn: &Connection, args: ImportArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();

    // Determine paths to import
    let import_paths: Vec<PathBuf> = if args.paths.is_empty() {
        let cwd = std::env::current_dir().map_err(Error::Io)?;
        validate_current_dir(&cwd, &config, args.interactive)?;
        vec![cwd]
    } else {
        args.paths.clone()
    };

    let mut imported = 0usize;
    let mut skipped = 0usize;

    for raw_path in import_paths {
        match process_one(conn, &raw_path, &args, &config, now)? {
            OneResult::Skipped(existing_id) => {
                println!(
                    "skipped: {} already imported as #{}",
                    raw_path.display(),
                    existing_id
                );
                skipped += 1;
            }
            OneResult::DryRun { slug, status, path } => {
                println!(
                    "[dry-run] would import  {}  ({})  {}",
                    slug,
                    status,
                    path.display()
                );
                imported += 1;
            }
            OneResult::Done(res) => {
                println!(
                    "imported #{}  {}  ({})  {}",
                    res.order_id,
                    res.slug,
                    res.status,
                    res.path.display()
                );
                imported += 1;
            }
            OneResult::Error(msg) => {
                eprintln!("error: {msg}");
                // continue to next path
            }
        }
    }

    println!();
    println!(
        "imported {} orders (skipped {} duplicates)",
        imported, skipped
    );
    Ok(())
}

enum OneResult {
    Skipped(i64),
    DryRun {
        slug: String,
        status: OrderStatus,
        path: PathBuf,
    },
    Done(ImportResult),
    Error(String),
}

fn process_one(
    conn: &Connection,
    raw_path: &Path,
    args: &ImportArgs,
    config: &Config,
    now: i64,
) -> Result<OneResult> {
    // Basic validation
    if !raw_path.exists() {
        return Ok(OneResult::Error(format!(
            "{} does not exist",
            raw_path.display()
        )));
    }
    if !raw_path.is_dir() {
        return Ok(OneResult::Error(format!(
            "{} is not a directory",
            raw_path.display()
        )));
    }

    // Warn if not git root
    if !raw_path.join(".git").exists() {
        eprintln!(
            "warning: {} is not a git repository root",
            raw_path.display()
        );
    }

    // Canonicalize for duplicate check
    let canonical = match std::fs::canonicalize(raw_path) {
        Ok(p) => p,
        Err(e) => {
            return Ok(OneResult::Error(format!(
                "cannot canonicalize {}: {e}",
                raw_path.display()
            )));
        }
    };

    // Check for duplicate before doing anything else
    let canonical_str = canonical.to_string_lossy().to_string();
    if let Some(existing_id) =
        gig_core::services::import::is_already_imported(conn, &canonical_str)?
    {
        return Ok(OneResult::Skipped(existing_id));
    }

    // Infer metadata
    let dir_name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (date_ts, rest) = parse_date_prefix(&dir_name);
    let base_slug = slugify(&rest);
    let base_slug = if base_slug.is_empty() {
        "import".to_string()
    } else {
        base_slug
    };

    let inferred_status = match &args.status {
        Some(s) => {
            OrderStatus::from_str(s).map_err(|_| Error::Invalid(format!("unknown status: {s}")))?
        }
        None => infer_status(
            &canonical,
            &config.general.dev_root,
            &config.general.archive_root,
        ),
    };
    let inferred_created_at = infer_created_at(&canonical, date_ts);
    let inferred_title = slug_to_title(&base_slug);

    // Build input (possibly overridden interactively)
    let mut input = ImportInput {
        path: raw_path.to_path_buf(),
        slug_override: None,
        title_override: None,
        status_override: args
            .status
            .as_deref()
            .and_then(|s| OrderStatus::from_str(s).ok()),
        quoted_price: None,
        final_price: None,
        client_name: None,
        source_org: None,
        notes: None,
        tags: vec![],
    };

    if args.interactive {
        interactive_prompt(
            &canonical,
            &base_slug,
            &inferred_title,
            inferred_status,
            inferred_created_at,
            &mut input,
        )?;
    }

    if args.dry_run {
        // Compute what the slug would be
        let slug_preview = input.slug_override.clone().unwrap_or(base_slug);
        let status_preview = input.status_override.unwrap_or(inferred_status);
        return Ok(OneResult::DryRun {
            slug: slug_preview,
            status: status_preview,
            path: canonical,
        });
    }

    let result = import_project(conn, &input, config, now)?;

    if let Some(existing_id) = result.skipped_as {
        return Ok(OneResult::Skipped(existing_id));
    }

    // Relocate if requested
    if args.relocate {
        let slug = &result.slug;
        let target = if result.status == OrderStatus::Archived {
            config.general.archive_root.join(slug)
        } else {
            config.general.dev_root.join(slug)
        };
        match relocate_project(conn, result.order_id, &target) {
            Ok(new_path) => {
                println!("relocated to {}", new_path.display());
            }
            Err(e) => {
                eprintln!("relocate failed: {e}");
            }
        }
    }

    Ok(OneResult::Done(result))
}

fn validate_current_dir(cwd: &Path, config: &Config, interactive: bool) -> Result<()> {
    // Guard against importing dev_root or archive_root themselves
    let canon_cwd = std::fs::canonicalize(cwd).map_err(Error::Io)?;

    if let Ok(dev_canon) = std::fs::canonicalize(&config.general.dev_root) {
        if canon_cwd == dev_canon {
            return Err(Error::Invalid(
                "refusing to import dev_root itself as a project".into(),
            ));
        }
    }
    if let Ok(arch_canon) = std::fs::canonicalize(&config.general.archive_root) {
        if canon_cwd == arch_canon {
            return Err(Error::Invalid(
                "refusing to import archive_root itself as a project".into(),
            ));
        }
    }

    // Warn / confirm if not a git root
    if !cwd.join(".git").exists() {
        eprint!("not a git root, are you sure? [y/N] ");
        io::stderr().flush().ok();
        if interactive {
            // In interactive mode we ask anyway below; just warn
        } else {
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line).map_err(Error::Io)?;
            if !matches!(line.trim(), "y" | "Y") {
                return Err(Error::Invalid("aborted".into()));
            }
        }
    }
    Ok(())
}

/// Prompt the user for each metadata field, keeping inferred values on empty input.
fn interactive_prompt(
    path: &Path,
    base_slug: &str,
    inferred_title: &str,
    inferred_status: OrderStatus,
    _inferred_created_at: i64,
    input: &mut ImportInput,
) -> Result<()> {
    println!("importing: {}", path.display());

    let slug = prompt_field("slug", base_slug)?;
    if !slug.is_empty() {
        input.slug_override = Some(slug.clone());
    }
    let effective_slug = input.slug_override.as_deref().unwrap_or(base_slug);
    let default_title = if input.slug_override.is_some() {
        slug_to_title(effective_slug)
    } else {
        inferred_title.to_string()
    };

    let title = prompt_field("title", &default_title)?;
    if !title.is_empty() {
        input.title_override = Some(title);
    }

    let status_str = prompt_field("status", inferred_status.as_str())?;
    if !status_str.is_empty() {
        match OrderStatus::from_str(&status_str) {
            Ok(s) => input.status_override = Some(s),
            Err(_) => eprintln!("  unknown status '{status_str}', keeping inferred"),
        }
    }

    let qp = prompt_field("quoted_price", "—")?;
    if !qp.is_empty() && qp != "—" {
        input.quoted_price = qp.parse::<i64>().ok();
    }

    let fp = prompt_field("final_price", "—")?;
    if !fp.is_empty() && fp != "—" {
        input.final_price = fp.parse::<i64>().ok();
    }

    let client = prompt_field("client", "—")?;
    if !client.is_empty() && client != "—" {
        input.client_name = Some(client);
    }

    let src_org = prompt_field("source_org", "—")?;
    if !src_org.is_empty() && src_org != "—" {
        input.source_org = Some(src_org);
    }

    let notes = prompt_field("notes", "—")?;
    if !notes.is_empty() && notes != "—" {
        input.notes = Some(notes);
    }

    let tags = prompt_field("tags", "—")?;
    if !tags.is_empty() && tags != "—" {
        input.tags = tags
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
    }

    Ok(())
}

fn prompt_field(name: &str, default: &str) -> Result<String> {
    print!("  {name:<12} [{default}]: ");
    io::stdout().flush().map_err(Error::Io)?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).map_err(Error::Io)?;
    Ok(line.trim().to_string())
}

fn slug_to_title(slug: &str) -> String {
    slug.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
