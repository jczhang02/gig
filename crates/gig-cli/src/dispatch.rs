//! Command dispatch and DB bootstrap.

use crate::cli::{Cli, Command};
use crate::commands;
use gig_core::{config::Paths, db, Result};

pub fn run(cli: Cli) -> Result<()> {
    if let Command::Completion(args) = cli.command {
        return commands::completion::run(args);
    }
    if let Command::Gui(args) = cli.command {
        return gig_gui::run(gig_gui::GuiOptions {
            port: args.port,
            no_open: args.no_open,
        });
    }

    let paths = Paths::from_env()?;
    paths.ensure_dirs()?;
    let conn = db::open(&paths.db_file)?;

    match cli.command {
        Command::New(args) => commands::new::run(&conn, args),
        Command::Ls(args) => commands::ls::run(&conn, args),
        Command::Show(args) => commands::show::run(&conn, args),
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
        Command::Doctor(args) => commands::doctor::run(&conn, args),
        Command::Stats(args) => commands::stats::run(&conn, args),
        Command::Export(args) => commands::export::run(&conn, args),
        Command::Client(args) => commands::client::run(&conn, args),
        Command::Template(args) => commands::template::run(&conn, args),
        Command::Config(args) => commands::config::run(&conn, args),
        Command::Import(args) => commands::import::run(&conn, args),
        Command::Backup => commands::backup::run(&conn),
        Command::Source(args) => commands::source::run(&conn, args),
        Command::Quote(args) => commands::quote::run(&conn, args),
        Command::Plan(args) => commands::plan::run(&conn, args),
        Command::Work(args) => commands::work::run(&conn, args),
        Command::Acceptance(args) => commands::acceptance::run(&conn, args),
        Command::Package(args) => commands::package::run(&conn, args),
        Command::Artifact(args) => commands::artifact::run(&conn, args),
        Command::Serve(args) => commands::serve::run(&conn, args),
        Command::Delete(args) => commands::delete::run(&conn, args),
        Command::Completion(_) => unreachable!("completion is handled before DB bootstrap"),
        Command::Gui(_) => unreachable!("gui is handled before DB bootstrap"),
    }
}
