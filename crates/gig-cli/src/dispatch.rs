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
        Command::Init(args) => commands::init::run(&conn, args),
        Command::Price(args) => commands::price::run(&conn, args),
        Command::Change(args) => commands::change::run(&conn, args),
        Command::Note(args) => commands::note::run(&conn, args),
        Command::Tag(args) => commands::tag::run(&conn, args),
        Command::Cut(args) => commands::cut::run(&conn, args),
        Command::Status(args) => commands::status::run(&conn, args),
        Command::Paid(args) => commands::paid::run(&conn, args),
        Command::Archive(args) => commands::archive::run(&conn, args),
        Command::Lead(args) => commands::lead::run(&conn, args),
        Command::Cd(args) => commands::cd::run(&conn, args),
        Command::Doctor => commands::doctor::run(&conn),
        Command::Pack(args) => commands::pack::run(&conn, args),
        Command::Deliver(args) => commands::deliver::run(&conn, args),
        Command::Stats(args) => commands::stats::run(&conn, args),
        Command::Export(args) => commands::export::run(&conn, args),
        Command::Client(args) => commands::client::run(&conn, args),
        Command::Template(args) => commands::template::run(&conn, args),
        Command::Config(args) => commands::config::run(&conn, args),
        Command::Backup => commands::backup::run(&conn),
    }
}
