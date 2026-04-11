use crate::cli::LsArgs;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(_conn: &Connection, _args: LsArgs) -> Result<()> {
    unimplemented!("task 5.5")
}
