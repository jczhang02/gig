//! Unified error type for gig-core.

use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("migration error: {0}")]
    Migration(#[from] refinery::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("toml parse error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("order not found: {0}")]
    OrderNotFound(String),

    #[error("invalid status transition: {from} → {to}")]
    InvalidTransition { from: String, to: String },

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("path {0} is not accessible: {1}")]
    PathUnavailable(PathBuf, io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_contains_source_message() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "nope");
        let err: Error = io_err.into();
        assert!(err.to_string().contains("nope"));
    }

    #[test]
    fn order_not_found_formats_id() {
        let err = Error::OrderNotFound("42".into());
        assert_eq!(err.to_string(), "order not found: 42");
    }

    #[test]
    fn invalid_transition_formats_both_sides() {
        let err = Error::InvalidTransition {
            from: "lead".into(),
            to: "delivered".into(),
        };
        assert_eq!(
            err.to_string(),
            "invalid status transition: lead → delivered"
        );
    }
}
