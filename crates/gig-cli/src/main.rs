use clap::Parser;

mod cli;
mod commands;
mod dispatch;
mod ui;

fn main() {
    let args = cli::Cli::parse();
    if let Err(e) = dispatch::run(args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
