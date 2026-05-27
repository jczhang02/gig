use crate::cli::{Cli, CompletionArgs, CompletionShell};
use clap::CommandFactory;
use clap_complete::aot::{generate, Shell};
use gig_core::Result;

pub fn run(args: CompletionArgs) -> Result<()> {
    let shell = match args.shell {
        CompletionShell::Zsh => Shell::Zsh,
    };

    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "gig", &mut std::io::stdout());
    Ok(())
}
