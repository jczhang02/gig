use crate::cli::StatsArgs;
use crate::ui::format_price;
use gig_core::services::stats::{
    compute_stats, current_month_bounds, now_unix, parse_month_range, GroupBy,
};
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: StatsArgs) -> Result<()> {
    let range = match args.range.as_deref() {
        Some(s) => {
            let r = parse_month_range(s).ok_or_else(|| {
                gig_core::Error::Invalid(
                    "range must be YYYY-MM..YYYY-MM, e.g. 2024-01..2024-03".into(),
                )
            })?;
            Some(r)
        }
        None => None,
    };

    let group_by = match args.by.as_deref() {
        Some("tag") => Some(GroupBy::Tag),
        Some("client") => Some(GroupBy::Client),
        Some(other) => {
            return Err(gig_core::Error::Invalid(format!(
                "unknown group: {other:?}; use tag or client"
            )))
        }
        None => None,
    };

    let now = now_unix();
    let effective_range = range.unwrap_or_else(|| current_month_bounds(now));

    let stats = compute_stats(conn, Some(effective_range), group_by)?;

    println!("── stats ─────────────────────────────────────");
    println!(
        "  total income : {}",
        format_price(Some(stats.total_income), "CNY")
    );
    println!("  orders       : {}", stats.order_count);
    if let Some(avg) = stats.avg_order_value {
        println!("  avg value    : {}", format_price(Some(avg), "CNY"));
    }
    if let Some(rate) = stats.lead_to_accepted_rate {
        println!("  acceptance   : {:.1}%", rate * 100.0);
    }

    if !stats.breakdown.is_empty() {
        println!();
        println!("  breakdown:");
        for (name, income, count) in &stats.breakdown {
            println!(
                "    {:<20} {:>12}  ({} orders)",
                name,
                format_price(Some(*income), "CNY"),
                count
            );
        }
    }

    Ok(())
}
