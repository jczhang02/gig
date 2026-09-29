//! $EDITOR round trips for multi-line fields, notes, and JOB.md. The caller
//! suspends the terminal around these (`terminal::suspend_while`).

use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

/// `$VISUAL`, then `$EDITOR`, then `vi`, split on whitespace so values like
/// `code -w` work.
pub fn command() -> Vec<String> {
    ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .map(|v| v.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .find(|v| !v.is_empty())
        .unwrap_or_else(|| vec!["vi".to_string()])
}

/// Edit `path` in place and wait for the editor to exit.
pub fn edit_file(path: &Path) -> io::Result<()> {
    let cmd = command();
    let status = Command::new(&cmd[0]).args(&cmd[1..]).arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{} exited with {status}", cmd[0])))
    }
}

/// Edit `initial` in a temporary Markdown file and return the new text with
/// trailing whitespace removed. The file is deleted afterwards.
pub fn edit_text(initial: &str) -> io::Result<String> {
    let mut file = tempfile::Builder::new()
        .prefix("gig-")
        .suffix(".md")
        .tempfile()?;
    file.write_all(initial.as_bytes())?;
    if !initial.is_empty() && !initial.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    file.flush()?;
    edit_file(file.path())?;
    let text = std::fs::read_to_string(file.path())?;
    Ok(text.trim_end().to_string())
}
