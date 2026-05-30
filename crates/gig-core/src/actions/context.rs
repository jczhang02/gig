use crate::config::{Config, Paths};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Explicit dependencies for one synchronous action execution.
///
/// UI adapters should build this per request/invocation, execute one action, and
/// then drop the SQLite connection. It is intentionally not cloneable and should
/// not be stored in async server state.
pub struct ActionContext<'a> {
    pub conn: &'a Connection,
    pub paths: &'a Paths,
    pub config: &'a Config,
    pub cwd: &'a Path,
    pub now: i64,
}

impl<'a> ActionContext<'a> {
    pub fn new(
        conn: &'a Connection,
        paths: &'a Paths,
        config: &'a Config,
        cwd: &'a Path,
        now: i64,
    ) -> Self {
        Self {
            conn,
            paths,
            config,
            cwd,
            now,
        }
    }
}

/// Owned inputs that adapters can clone into a blocking action executor before
/// constructing an `ActionContext` with a request-local database connection.
#[derive(Debug, Clone)]
pub struct ActionContextSeed {
    pub paths: Paths,
    pub config: Config,
    pub cwd: PathBuf,
    pub now: i64,
}
