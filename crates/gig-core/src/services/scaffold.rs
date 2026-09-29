//! Day-zero project files, rendered from the templates directory.

use crate::config::Config;
use crate::models::Order;
use crate::templates;
use crate::{clock, Error, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Scaffolded {
    pub created: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

/// Draft notes are embedded under a `##` heading in JOB.md: drop the notes' own
/// title line and push every other heading one level down.
pub fn demote_notes(notes: &str) -> String {
    let mut out = Vec::new();
    for (i, line) in notes.lines().enumerate() {
        if i == 0 && line.starts_with("# ") {
            continue;
        }
        if line.starts_with('#') {
            out.push(format!("#{line}"));
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n").trim().to_string()
}

pub fn template_context(
    order: &Order,
    draft_notes: Option<&str>,
    warranty_days: i64,
) -> serde_json::Value {
    let draft_notes = draft_notes.map(demote_notes);
    serde_json::json!({
        "slug": order.slug,
        "title": order.title,
        "project_type": order.project_type.as_str(),
        "material_path": order.material_path,
        "platform": order.platform,
        "external_id": order.external_id,
        "currency": order.currency,
        "price": order.price,
        "cut_ratio": order.cut_ratio,
        "client_words": order.client_words,
        "dev_path": order.dev_path,
        "today": clock::today(),
        "draft_notes": draft_notes,
        "warranty_days": warranty_days,
    })
}

/// Create the project directory and its files. Refuses if `dev_path` exists.
pub fn create(config: &Config, order: &Order, draft_notes: Option<&str>) -> Result<Scaffolded> {
    let dev_path = PathBuf::from(
        order
            .dev_path
            .as_deref()
            .ok_or_else(|| Error::InvalidState("order has no dev_path".into()))?,
    );
    if dev_path.exists() {
        return Err(Error::InvalidInput(format!(
            "{} already exists; use --adopt to register an existing directory",
            dev_path.display()
        )));
    }
    let tdir = &config.general.templates_dir;
    let missing = templates::missing(tdir);
    if !missing.is_empty() {
        return Err(Error::Config(format!(
            "templates missing from {}: {}",
            tdir.display(),
            missing.join(", ")
        )));
    }
    let ctx = template_context(order, draft_notes, config.general.warranty_days);
    let agents = templates::agents_template(tdir, order.project_type.as_str());
    let files: [(&str, PathBuf); 5] = [
        (".gig/JOB.md", PathBuf::from("JOB.md.j2")),
        (".gig/QUOTE.md", PathBuf::from("QUOTE.md.j2")),
        ("AGENTS.md", agents),
        ("README.md", PathBuf::from("README.md.j2")),
        (".gitignore", PathBuf::from("gitignore")),
    ];
    let mut created = Vec::new();
    std::fs::create_dir_all(dev_path.join(".gig"))
        .map_err(|e| Error::PathUnavailable(dev_path.join(".gig"), e))?;
    for (target, template) in files {
        let text = templates::render(tdir, &template.to_string_lossy(), &ctx)?;
        let p = dev_path.join(target);
        std::fs::write(&p, text).map_err(|e| Error::PathUnavailable(p.clone(), e))?;
        created.push(p);
    }
    for dir in ["data", "references"] {
        let p = dev_path.join(dir);
        std::fs::create_dir_all(&p).map_err(|e| Error::PathUnavailable(p.clone(), e))?;
        created.push(p);
    }
    let mut warnings = Vec::new();
    match Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&dev_path)
        .output()
    {
        Ok(out) if out.status.success() => {
            let _ = Command::new("git")
                .args([
                    "-C",
                    &dev_path.to_string_lossy(),
                    "checkout",
                    "-q",
                    "-B",
                    "main",
                ])
                .output();
        }
        Ok(out) => warnings.push(format!(
            "git init failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => warnings.push(format!(
            "git not available ({e}); repository not initialised"
        )),
    }
    Ok(Scaffolded { created, warnings })
}

/// Files that should exist in an adopted project.
pub fn adoption_warnings(dev_path: &Path) -> Vec<String> {
    let mut w = Vec::new();
    for f in [".gig/JOB.md", ".gig/QUOTE.md"] {
        if !dev_path.join(f).is_file() {
            w.push(format!("{f} is missing"));
        }
    }
    if !dev_path.join(".git").exists() {
        w.push("not a git repository".into());
    }
    w
}

#[cfg(test)]
pub(crate) fn write_test_templates(dir: &Path) {
    std::fs::write(
        dir.join("NOTES.md.j2"),
        "# {{ slug }}\n\nmaterial: {{ material_path }}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("JOB.md.j2"),
        "# {{ title }}\n\n- slug: {{ slug }}\n{% if draft_notes %}\n## Draft notes\n\n{{ draft_notes }}{% endif %}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("QUOTE.md.j2"),
        "# Quote\n\n- price: {{ currency }} {{ price }}\n- accepted: {{ today }}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("AGENTS.md.j2"),
        "# Agents\n\nRead .gig/JOB.md first.\n",
    )
    .unwrap();
    std::fs::write(dir.join("README.md.j2"), "# {{ title }}\n").unwrap();
    std::fs::write(dir.join("gitignore"), ".venv/\ndelivery/\n.scratch/\n").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demotes_headings_and_drops_title() {
        let notes = "# slug: T\n\n- a\n\n## 客户原话\n\nx\n\n### deeper\n";
        assert_eq!(
            demote_notes(notes),
            "- a\n\n### 客户原话\n\nx\n\n#### deeper"
        );
    }
}
