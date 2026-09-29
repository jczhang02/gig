//! `gig`: one JSON document per invocation. See docs/v2/SPEC.md.

mod cli;
mod run;

use clap::Parser;
use serde_json::json;
use std::io::IsTerminal;

fn main() {
    let args = cli::Cli::parse();
    let Some(command) = &args.command else {
        bare(&args.tui);
    };
    if let cli::Command::Completion(c) = command {
        // The one raw-output command: `gig completion zsh > _gig`.
        let mut cmd = <cli::Cli as clap::CommandFactory>::command();
        clap_complete::generate(c.shell, &mut cmd, "gig", &mut std::io::stdout());
        return;
    }
    if let cli::Command::Tui(t) = command {
        dashboard(t, "gig tui");
    }
    let command = args.command_name();
    match run::run(args) {
        Ok(output) => {
            let doc = json!({
                "ok": true,
                "command": command,
                "data": output.data,
                "warnings": output.warnings,
            });
            println!("{}", serde_json::to_string_pretty(&doc).expect("json"));
        }
        Err(e) => {
            let doc = json!({
                "ok": false,
                "command": command,
                "error": { "code": e.code(), "message": e.to_string() },
            });
            println!("{}", serde_json::to_string_pretty(&doc).expect("json"));
            std::process::exit(1);
        }
    }
}

/// Bare `gig`: the dashboard in a terminal. Without one (agents, pipes) it
/// fails exactly as before the dashboard existed: clap's missing-subcommand
/// error, exit 2. `--list-themes` works anywhere.
fn bare(t: &cli::TuiArgs) -> ! {
    if !t.list_themes && !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        // Exits with clap's missing-subcommand error (help on stderr, exit 2).
        cli::CommandRequired::parse();
    }
    dashboard(t, "gig")
}

/// The second raw-output command: a terminal UI, no JSON envelope. Errors
/// go to stderr after `prefix`, the command as typed (`gig` or `gig tui`).
fn dashboard(t: &cli::TuiArgs, prefix: &str) -> ! {
    let result = if t.list_themes {
        gig_tui::list_themes(&mut std::io::stdout(), &mut std::io::stderr())
    } else {
        gig_tui::run(t.opts())
    };
    if let Err(e) = result {
        eprintln!("{prefix}: {e}");
        std::process::exit(1);
    }
    std::process::exit(0)
}
