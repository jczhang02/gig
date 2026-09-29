//! Business rules. Every CLI command maps to one function here.

pub mod archive;
pub mod artifacts;
pub mod doctor;
pub mod drafts;
pub mod migrate;
pub mod orders;
pub mod packages;
pub mod scaffold;
pub mod split_secrets;

use crate::config::{Config, Paths};
use crate::{db, Error, Result};
use rusqlite::Connection;

/// Everything a service call needs.
pub struct Ctx {
    pub paths: Paths,
    pub config: Config,
    pub conn: Connection,
}

impl Ctx {
    pub fn open() -> Result<Self> {
        let paths = Paths::from_env()?;
        paths.ensure_dirs()?;
        let config = Config::load(&paths.config_file)?;
        let conn = db::open(&paths.db_file)?;
        Ok(Self {
            paths,
            config,
            conn,
        })
    }

    /// Paths and config only, no database (migrate, split-secrets, config commands).
    pub fn without_db() -> Result<(Paths, Config)> {
        let paths = Paths::from_env()?;
        paths.ensure_dirs()?;
        let config = Config::load(&paths.config_file)?;
        Ok((paths, config))
    }

    #[cfg(test)]
    pub fn for_test(root: &std::path::Path) -> Self {
        let paths = Paths::under_root(root);
        paths.ensure_dirs().unwrap();
        let mut config = Config::default();
        config.general.dev_root = root.join("dev");
        config.general.archive_root = root.join("archive");
        config.general.drafts_dir = std::path::PathBuf::new();
        config.general.templates_dir = root.join("templates");
        std::fs::create_dir_all(&config.general.dev_root).unwrap();
        std::fs::create_dir_all(&config.general.templates_dir).unwrap();
        crate::services::scaffold::write_test_templates(&config.general.templates_dir);
        let conn = db::open_in_memory().unwrap();
        Self {
            paths,
            config,
            conn,
        }
    }
}

/// Slugs name directories and object keys: lowercase ASCII, digits, '-', '_', '.'.
pub fn validate_slug(slug: &str) -> Result<()> {
    let ok = !slug.is_empty()
        && slug.len() <= 64
        && slug
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && slug.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
        })
        && !slug.contains("..");
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "invalid slug {slug:?}: 1-64 lowercase ASCII letters, digits, '-', '_' or '.', starting with a letter or digit"
        )))
    }
}

/// Commands that change the world outside the database need `--yes`.
pub fn require_yes(yes: bool, what: &str) -> Result<()> {
    if yes {
        Ok(())
    } else {
        Err(Error::NeedsYes(what.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        validate_slug("tk-dtf-compact").unwrap();
        validate_slug("a1.b_c").unwrap();
        for bad in ["", "Abc", "-a", "a b", "a/b", "中文", "a..b"] {
            assert!(validate_slug(bad).is_err(), "{bad}");
        }
    }
}
