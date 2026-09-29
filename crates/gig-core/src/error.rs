//! One error type for the crate, with a stable machine-readable code.

use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid state: {0}")]
    InvalidState(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("unsafe package: {0}")]
    UnsafePackage(String),

    #[error("package must be checked first: {0}")]
    NeedsCheck(String),

    #[error("refusing without --yes: {0}")]
    NeedsYes(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("secrets error: {0}")]
    Secrets(String),

    #[error("upload failed: {0}")]
    Upload(String),

    #[error("legacy database: {0}")]
    LegacyDb(String),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("path {0} is not accessible: {1}")]
    PathUnavailable(PathBuf, io::Error),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("migration error: {0}")]
    Migration(#[from] refinery::Error),

    #[error("toml parse error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("template error: {0}")]
    Template(#[from] minijinja::Error),
}

impl Error {
    /// Stable code printed in the JSON envelope. Keep in sync with docs/v2/SPEC.md.
    pub fn code(&self) -> &'static str {
        match self {
            Error::NotFound(_) => "not_found",
            Error::InvalidState(_) => "invalid_state",
            Error::InvalidInput(_) => "invalid_input",
            Error::UnsafePackage(_) => "unsafe_package",
            Error::NeedsCheck(_) => "needs_check",
            Error::NeedsYes(_) => "needs_yes",
            Error::Config(_) | Error::TomlDe(_) | Error::TomlSer(_) | Error::Template(_) => {
                "config"
            }
            Error::Secrets(_) => "secrets",
            Error::Upload(_) => "upload",
            Error::LegacyDb(_) => "legacy_db",
            Error::Io(_) | Error::PathUnavailable(_, _) => "io",
            Error::Db(_) | Error::Migration(_) => "db",
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable() {
        assert_eq!(Error::NotFound("x".into()).code(), "not_found");
        assert_eq!(Error::UnsafePackage("x".into()).code(), "unsafe_package");
        assert_eq!(Error::NeedsYes("x".into()).code(), "needs_yes");
        let io: Error = io::Error::new(io::ErrorKind::NotFound, "nope").into();
        assert_eq!(io.code(), "io");
        assert!(io.to_string().contains("nope"));
    }
}
