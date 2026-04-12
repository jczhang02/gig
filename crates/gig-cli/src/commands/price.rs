use crate::cli::PriceArgs;
use crate::commands::resolve_order;
use crate::ui::{self, parse_yuan};
use gig_core::services::lifecycle::change_price;
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: PriceArgs) -> Result<()> {
    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let amount_cents = parse_yuan(&args.amount)?;
    let updated = change_price(conn, order.id, amount_cents, args.reason.as_deref(), now)?;
    println!("updated price for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}
