use crate::cli::ChangeArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::services::lifecycle::add_requirement_change;
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: ChangeArgs) -> Result<()> {
    // Parse delta from yuan (may be negative, e.g. "-100")
    let delta_cents = parse_delta_yuan(&args.delta)?;

    let order = resolve_order(args.id, conn)?;
    ui::print_banner(&order);
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let updated = add_requirement_change(conn, order.id, &args.message, delta_cents, now)?;
    println!("recorded requirement change for order #{}", updated.id);
    println!("{}", ui::order_detail(&updated));
    Ok(())
}

/// Parse a delta string in yuan (possibly negative) to minor units (cents).
/// "300" → 30000, "-100" → -10000, "50.5" → 5050
fn parse_delta_yuan(s: &str) -> Result<i64> {
    let s = s.trim();
    let (negative, digits) = if let Some(rest) = s.strip_prefix('-') {
        (true, rest)
    } else {
        (false, s)
    };
    let abs_cents = crate::ui::parse_yuan(digits)?;
    if negative && abs_cents != 0 {
        Ok(-abs_cents)
    } else {
        Ok(abs_cents)
    }
}
