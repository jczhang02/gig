use crate::cli::TagArgs;
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::services::lifecycle::add_tags;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: TagArgs) -> Result<()> {
    let order = find_by_id_or_slug(conn, &args.id)?;
    let tag_refs: Vec<&str> = args.tags.iter().map(|s| s.as_str()).collect();
    let tags = add_tags(conn, order.id, &tag_refs)?;
    println!(
        "tags for order #{}: {}",
        order.id,
        tags.iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}
