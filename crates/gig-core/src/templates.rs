//! Project files are rendered from templates on disk (`general.templates_dir`),
//! never from strings compiled into the binary, so the skill can edit them.

use crate::{Error, Result};
use minijinja::Environment;
use std::path::{Path, PathBuf};

/// Template files the scaffold needs. `AGENTS.<type>.md.j2` is optional per type.
pub const REQUIRED: &[&str] = &[
    "NOTES.md.j2",
    "JOB.md.j2",
    "QUOTE.md.j2",
    "AGENTS.md.j2",
    "README.md.j2",
    "gitignore",
];

pub fn missing(templates_dir: &Path) -> Vec<String> {
    REQUIRED
        .iter()
        .filter(|name| !templates_dir.join(name).is_file())
        .map(|s| s.to_string())
        .collect()
}

/// Render `name` (a file in `templates_dir`) with `ctx`.
pub fn render(templates_dir: &Path, name: &str, ctx: &serde_json::Value) -> Result<String> {
    let path = templates_dir.join(name);
    let source = std::fs::read_to_string(&path)
        .map_err(|e| Error::Config(format!("template {} not readable ({e})", path.display())))?;
    let mut env = Environment::new();
    env.set_keep_trailing_newline(true);
    env.add_template(name, &source)?;
    let tmpl = env.get_template(name)?;
    Ok(tmpl.render(ctx)?)
}

/// `AGENTS.<type>.md.j2` when it exists, else `AGENTS.md.j2`.
pub fn agents_template(templates_dir: &Path, project_type: &str) -> PathBuf {
    let typed = format!("AGENTS.{project_type}.md.j2");
    if templates_dir.join(&typed).is_file() {
        PathBuf::from(typed)
    } else {
        PathBuf::from("AGENTS.md.j2")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("JOB.md.j2"),
            "# {{ title }}\n\n{{ slug }}\n",
        )
        .unwrap();
        let out = render(
            dir.path(),
            "JOB.md.j2",
            &serde_json::json!({"title": "T", "slug": "s"}),
        )
        .unwrap();
        assert_eq!(out, "# T\n\ns\n");
        assert!(missing(dir.path()).contains(&"QUOTE.md.j2".to_string()));
        assert_eq!(
            agents_template(dir.path(), "tool"),
            PathBuf::from("AGENTS.md.j2")
        );
        std::fs::write(dir.path().join("AGENTS.tool.md.j2"), "").unwrap();
        assert_eq!(
            agents_template(dir.path(), "tool"),
            PathBuf::from("AGENTS.tool.md.j2")
        );
    }
}
