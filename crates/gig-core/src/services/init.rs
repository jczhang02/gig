//! `gig init` service: scaffold a project directory for an accepted order.

use crate::models::{Order, OrderStatus};
use crate::repo::orders as repo;
use crate::services::orders::transition;
use crate::templates::render_project_readme;
use crate::{Error, Result};
use rusqlite::Connection;
use std::path::Path;
use std::process::Command;

/// Standard .gitignore content for Rust / Node / Python projects.
const GITIGNORE: &str = r#"# Rust
target/
Cargo.lock

# Node
node_modules/
dist/
.next/
.nuxt/

# Python
__pycache__/
*.py[cod]
.venv/
venv/
*.egg-info/
dist/
build/

# Editor
.idea/
.vscode/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db
"#;

/// Generate a slug from a title: lowercase, spaces → hyphens, strip non-ASCII-alphanumeric.
pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c == ' ' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Initialise a project folder for the given order.
///
/// * Validates status == Accepted
/// * Generates a slug if absent
/// * Creates `<dev_root>/<slug>/` and runs `git init` in it
/// * Writes `.gitignore` and `README.md`
/// * Stores the canonicalized path back in the DB
/// * Transitions status → InProgress
pub fn init_project(
    conn: &Connection,
    id: i64,
    dev_root: &Path,
    slug_override: Option<&str>,
) -> Result<Order> {
    let order = repo::find_by_id(conn, id)?;

    if order.status != OrderStatus::Accepted {
        return Err(Error::Invalid(format!(
            "order #{id} must be in 'accepted' state to init (current: {})",
            order.status
        )));
    }

    // Determine slug
    let slug = match slug_override {
        Some(s) => s.to_string(),
        None => match &order.slug {
            Some(s) => s.clone(),
            None => slugify(&order.title),
        },
    };

    // Persist slug if it changed
    if order.slug.as_deref() != Some(slug.as_str()) {
        conn.execute(
            "UPDATE orders SET slug = ?1 WHERE id = ?2",
            rusqlite::params![slug, id],
        )?;
    }

    // Create project directory
    let project_dir = dev_root.join(&slug);
    std::fs::create_dir_all(&project_dir)
        .map_err(|e| Error::PathUnavailable(project_dir.clone(), e))?;

    // git init
    let git_status = Command::new("git")
        .arg("init")
        .current_dir(&project_dir)
        .status()
        .map_err(Error::Io)?;
    if !git_status.success() {
        return Err(Error::Invalid(format!(
            "git init failed in {}",
            project_dir.display()
        )));
    }

    // Write .gitignore
    std::fs::write(project_dir.join(".gitignore"), GITIGNORE).map_err(Error::Io)?;

    // Reload order with updated slug for template rendering
    let order_for_tmpl = repo::find_by_id(conn, id)?;
    let readme = render_project_readme(&order_for_tmpl)?;
    std::fs::write(project_dir.join("README.md"), readme).map_err(Error::Io)?;

    // Canonicalize path and store dev_path
    let canonical = std::fs::canonicalize(&project_dir)
        .map_err(|e| Error::PathUnavailable(project_dir.clone(), e))?;
    let dev_path_str = canonical
        .to_str()
        .ok_or_else(|| Error::Invalid("project path is not valid UTF-8".into()))?
        .to_string();
    conn.execute(
        "UPDATE orders SET dev_path = ?1 WHERE id = ?2",
        rusqlite::params![dev_path_str, id],
    )?;

    // Transition accepted → in_progress
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    transition(conn, id, OrderStatus::InProgress, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::services::orders::{create_order, CreateOrderInput};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn accepted_order(conn: &Connection, title: &str) -> Order {
        create_order(
            conn,
            &CreateOrderInput {
                title,
                slug: None,
                client_id: None,
                source_org: None,
                source_id: None,
                quoted_price: Some(10_000),
                final_price: None,
                my_cut_ratio: 0.6,
                currency: "CNY",
                notes: Some("test notes"),
                as_lead: false,
            },
            1_000,
        )
        .unwrap()
    }

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
    }

    #[test]
    fn slugify_strips_non_ascii() {
        assert_eq!(slugify("Foo Bar! Baz"), "foo-bar-baz");
    }

    #[test]
    fn slugify_collapses_multiple_hyphens() {
        assert_eq!(slugify("foo  bar"), "foo-bar");
    }

    #[test]
    fn init_project_creates_dir_and_transitions() {
        let tmp = TempDir::new().unwrap();
        let conn = open_in_memory().unwrap();
        let order = accepted_order(&conn, "My Test Project");
        let result = init_project(&conn, order.id, tmp.path(), None).unwrap();
        assert_eq!(result.status, OrderStatus::InProgress);
        assert!(result.dev_path.is_some());
        let dev_path = PathBuf::from(result.dev_path.unwrap());
        assert!(dev_path.exists());
        assert!(dev_path.join(".gitignore").exists());
        assert!(dev_path.join("README.md").exists());
    }

    #[test]
    fn init_project_rejects_non_accepted() {
        let tmp = TempDir::new().unwrap();
        let conn = open_in_memory().unwrap();
        // Lead order (not accepted)
        let order = create_order(
            &conn,
            &CreateOrderInput {
                title: "lead order",
                slug: None,
                client_id: None,
                source_org: None,
                source_id: None,
                quoted_price: None,
                final_price: None,
                my_cut_ratio: 0.6,
                currency: "CNY",
                notes: None,
                as_lead: true,
            },
            0,
        )
        .unwrap();
        let err = init_project(&conn, order.id, tmp.path(), None).unwrap_err();
        assert!(err.to_string().contains("accepted"), "err: {err}");
    }
}
