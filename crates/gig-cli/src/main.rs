//! `gig`: one JSON document per invocation. See docs/v2/SPEC.md.

mod cli;
mod run;

use clap::Parser;
use serde_json::json;

fn main() {
    let args = cli::Cli::parse();
    if let cli::Command::Completion(c) = &args.command {
        // The one raw-output command: `gig completion zsh > _gig`.
        let mut cmd = <cli::Cli as clap::CommandFactory>::command();
        clap_complete::generate(c.shell, &mut cmd, "gig", &mut std::io::stdout());
        return;
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
