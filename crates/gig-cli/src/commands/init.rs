use crate::cli::InitArgs;
use crate::ui;
use gig_core::config::{Config, Paths};
use gig_core::context::canonical;
use gig_core::models::OrderStatus;
use gig_core::repo::orders::{self, find_by_id_or_slug};
use gig_core::repo::sources;
use gig_core::services::init::init_project;
use gig_core::services::orders::{create_order, CreateOrderInput};
use gig_core::{Error, Result};
use owo_colors::OwoColorize;
use rusqlite::Connection;
use std::io::{self, BufRead, Write as IoWrite};

pub fn run(conn: &Connection, args: InitArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    match args.id {
        Some(ref id_str) => {
            // Mode 1: existing order — `gig init <id>`
            let order = find_by_id_or_slug(conn, id_str)?;
            let dev_root = &config.general.dev_root;
            let order = init_project(conn, order.id, dev_root, args.slug.as_deref())?;
            println!("initialised project for order #{}", order.id);
            println!("  dev_path : {}", order.dev_path.as_deref().unwrap_or("—"));
            println!("{}", ui::order_detail(&order));
        }
        None => {
            // Mode 2: create new order for current directory — `gig init`
            let cwd = std::env::current_dir().map_err(Error::Io)?;
            let canonical_path = canonical(&cwd)?;
            let canonical_str = canonical_path.to_string_lossy().to_string();

            // Check not already registered
            if let Some(existing_id) =
                gig_core::services::import::is_already_imported(conn, &canonical_str)?
            {
                return Err(Error::Invalid(format!(
                    "this directory is already registered as order #{existing_id}"
                )));
            }

            // Derive slug from directory name or --slug
            let dir_name = canonical_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let slug = args
                .slug
                .unwrap_or_else(|| gig_core::services::import::slugify(&dir_name));

            // Get title
            let title = if let Some(t) = args.title {
                t
            } else if args.interactive {
                prompt_field("title", &titlecase(&slug))
            } else {
                titlecase(&slug)
            };

            // Get price
            let quoted_price = if let Some(ref p) = args.quoted_price {
                Some(ui::parse_yuan(p)?)
            } else if args.interactive {
                let input = prompt_field("quoted_price (yuan)", "");
                if input.is_empty() {
                    None
                } else {
                    Some(ui::parse_yuan(&input)?)
                }
            } else {
                None
            };

            // Get source
            let (source_id, cut_ratio) = if let Some(ref name) = args.source {
                let src = sources::find_by_name(conn, name)?
                    .ok_or_else(|| Error::Invalid(format!("source not found: {name}")))?;
                (Some(src.id), src.cut_ratio)
            } else if args.interactive {
                prompt_source(conn, &config)?
            } else {
                (None, config.general.default_cut_ratio)
            };

            let now = time::OffsetDateTime::now_utc().unix_timestamp();
            let input = CreateOrderInput {
                title: &title,
                slug: Some(&slug),
                client_id: None,
                source_org: None,
                source_id,
                quoted_price,
                final_price: quoted_price,
                my_cut_ratio: cut_ratio,
                currency: &config.general.default_currency,
                notes: None,
                as_lead: false,
            };
            let order = create_order(conn, &input, now)?;

            // Register current directory as dev_path, transition to in_progress
            orders::update_dev_path(conn, order.id, Some(&canonical_str))?;
            orders::update_status(
                conn,
                order.id,
                OrderStatus::InProgress,
                Some("accepted_at"),
                Some(now),
            )?;
            orders::update_status(conn, order.id, OrderStatus::InProgress, None, None)?;

            // git init if not already a git repo
            if !cwd.join(".git").exists() {
                let _ = std::process::Command::new("git")
                    .arg("init")
                    .current_dir(&cwd)
                    .status();
            }

            let order = orders::find_by_id(conn, order.id)?;
            println!(
                "{} created and initialised order #{}",
                "→".green().bold(),
                order.id
            );
            println!("  dev_path : {}", order.dev_path.as_deref().unwrap_or("—"));
            println!("{}", ui::order_detail(&order));
        }
    }
    Ok(())
}

fn titlecase(s: &str) -> String {
    s.split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn prompt_field(name: &str, default: &str) -> String {
    let default_display = if default.is_empty() {
        "—".to_string()
    } else {
        default.to_string()
    };
    print!("  {} [{}]: ", name.cyan().bold(), default_display.dimmed());
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).ok();
    let trimmed = line.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn prompt_source(conn: &Connection, config: &Config) -> Result<(Option<i64>, f64)> {
    let all_sources = sources::list(conn)?;
    if all_sources.is_empty() {
        return Ok((None, config.general.default_cut_ratio));
    }
    println!("  {} :", "source".cyan().bold());
    for (i, s) in all_sources.iter().enumerate() {
        println!(
            "    {}) {} ({}%)",
            (i + 1).to_string().dimmed(),
            s.name,
            (s.cut_ratio * 100.0) as u32
        );
    }
    println!("    {}) skip", "0".to_string().dimmed());
    print!("  select [{}]: ", "0".dimmed());
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).ok();
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed == "0" {
        return Ok((None, config.general.default_cut_ratio));
    }
    if let Ok(idx) = trimmed.parse::<usize>() {
        if idx >= 1 && idx <= all_sources.len() {
            let s = &all_sources[idx - 1];
            return Ok((Some(s.id), s.cut_ratio));
        }
    }
    Ok((None, config.general.default_cut_ratio))
}
