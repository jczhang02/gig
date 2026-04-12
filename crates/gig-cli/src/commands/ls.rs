use crate::cli::LsArgs;
use crate::ui;
use gig_core::models::OrderStatus;
use gig_core::repo::orders::{list, ListFilter};
use gig_core::services::dashboard::build_dashboard;
use gig_core::services::stats::now_unix;
use gig_core::{Error, Result};
use owo_colors::OwoColorize;
use rusqlite::Connection;
use std::str::FromStr;

pub fn run(conn: &Connection, args: LsArgs) -> Result<()> {
    if let Some(ref s) = args.status {
        // Filtered mode: plain table (original behaviour).
        let status = OrderStatus::from_str(s).map_err(|_| {
            Error::Invalid(format!(
                "unknown status {s:?}; valid: lead, negotiating, accepted, in_progress, delivered, paid, archived, cancelled"
            ))
        })?;
        let filter = ListFilter {
            status: Some(status),
        };
        let orders = list(conn, &filter)?;
        if orders.is_empty() {
            println!("no orders");
            return Ok(());
        }
        let table = ui::orders_table(&orders);
        println!("{table}");
    } else {
        // No filter: dashboard focus view.
        let now = now_unix();
        let dashboard = build_dashboard(conn, now)?;

        println!("{}", "今日应关注".bold());
        println!("{}", "─".repeat(60));

        if dashboard.focus_items.is_empty() {
            println!("  (nothing active)");
        } else {
            for item in &dashboard.focus_items {
                let o = &item.order;
                let slug_or_title = o.slug.as_deref().unwrap_or(&o.title);
                let price = ui::format_price(o.final_price.or(o.quoted_price), &o.currency);
                let status_str = ui::colour_status(o.status);

                let alert_part = match &item.alert {
                    Some(a) => format!("  {}", a.yellow()),
                    None => String::new(),
                };

                println!(
                    "  #{:<4}  {:<22}  {:<14}  {:<12}{}",
                    o.id, slug_or_title, status_str, price, alert_part
                );
            }
        }

        println!();
        let s = &dashboard.summary;
        println!(
            "  本月到手: {}  |  活跃: {}  |  待收款: {}",
            ui::format_price(Some(s.this_month_income), "CNY"),
            s.orders_count,
            s.pending_count,
        );
    }
    Ok(())
}
