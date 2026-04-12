use crate::cli::PriceArgs;
use crate::ui;
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::services::lifecycle::change_price;
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: PriceArgs) -> Result<()> {
    let order = find_by_id_or_slug(conn, &args.id)?;
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let updated = change_price(conn, order.id, args.amount, args.reason.as_deref(), now)?;
    println!("updated price for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
