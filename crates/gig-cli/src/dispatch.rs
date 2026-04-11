//! Command dispatch and DB bootstrap.

use crate::cli::{Cli, Command};
use crate::commands;
use gig_core::{config::Paths, db, Result};

pub fn run(cli: Cli) -> Result<()> {
    let paths = Paths::from_env()?;
    paths.ensure_dirs()?;
    let conn = db::open(&paths.db_file)?;

    match cli.command {
        Command::New(args) => commands::new::run(&conn, args),
        Command::Ls(args) => commands::ls::run(&conn, args),
        Command::Show(args) => commands::show::run(&conn, args),
    }
}
