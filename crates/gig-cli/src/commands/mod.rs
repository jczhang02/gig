pub mod archive;
pub mod backup;
pub mod cd;
pub mod change;
pub mod client;
pub mod config;
pub mod cut;
pub mod deliver;
pub mod doctor;
pub mod export;
pub mod init;
pub mod lead;
pub mod ls;
pub mod new;
pub mod note;
pub mod pack;
pub mod paid;
pub mod price;
pub mod show;
pub mod stats;
pub mod status;
pub mod tag;
pub mod template;

use gig_core::context::resolve_context;
use gig_core::models::Order;
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::{Error, Result};
use rusqlite::Connection;

/// Resolve an order from an optional CLI id/slug, falling back to context.
pub fn resolve_order(id: Option<String>, conn: &Connection) -> Result<Order> {
    match id {
        Some(ref s) => find_by_id_or_slug(conn, s),
        None => {
            let order_id = resolve_context(conn)?.ok_or_else(|| {
                Error::Invalid(
                    "not inside a gig project directory; pass <id> or cd into one.\n\
                     hint: `gig cd <id>` prints the dev_path."
                        .into(),
                )
            })?;
            gig_core::repo::orders::find_by_id(conn, order_id)
        }
    }
}
