//! `gig`: one JSON document per invocation. See docs/v2/SPEC.md.

mod cli;
mod run;

use clap::Parser;
use serde_json::json;

fn main() {
    let args = cli::Cli::parse();
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
