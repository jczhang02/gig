use crate::cli::DoctorArgs;
use gig_core::config::{Config, Paths};
use gig_core::context::{doctor_fix_paths, doctor_path_checks};
use gig_core::Result;
use owo_colors::OwoColorize;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: DoctorArgs) -> Result<()> {
    let diagnostics = doctor_path_checks(conn)?;

    if diagnostics.is_empty() {
        println!("{}", "all checks passed".green());
    } else {
        for d in &diagnostics {
            println!("  {}", d.yellow());
        }
        println!("\n{} issue(s) found", diagnostics.len());
    }

    if args.fix {
        let paths = Paths::from_env()?;
        let config = Config::load_or_default(&paths.config_file)?;
        let fixes = doctor_fix_paths(conn, &config.general.dev_root, &config.general.archive_root)?;
        if fixes.is_empty() {
            println!("{}", "no fixable paths found (check dev_root and archive_root in config)".dimmed());
        } else {
            println!();
            for f in &fixes {
                println!("  {}", f.green());
            }
            println!("\n{} path(s) fixed", fixes.len());
        }
    } else if !diagnostics.is_empty() {
        println!("{}", "hint: run `gig doctor --fix` to attempt automatic repair".dimmed());
    }

    Ok(())
}
