use crate::cli::LsArgs;
use crate::ui;
use gig_core::models::OrderStatus;
use gig_core::repo::orders::{list, ListFilter};
use gig_core::services::dashboard::build_dashboard;
use gig_core::services::stats::now_unix;
use gig_core::{Error, Result};
use owo_colors::OwoColorize;
use rusqlite::Connection;
use serde_json::json;
use std::str::FromStr;

pub fn run(conn: &Connection, args: LsArgs) -> Result<()> {
    if args.all {
        // Show every order as a flat table.
        let orders = list(conn, &ListFilter { status: None })?;
        if orders.is_empty() {
            println!("no orders");
        } else {
            println!("{}", ui::orders_table(&orders));
        }
        return Ok(());
    }

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
        if args.json {
            print_dashboard_json(&dashboard);
            return Ok(());
        }

        println!("{}", "Workflow decision board".bold());
        println!("{}", "─".repeat(60));

        for item in &dashboard.quote_items {
            let draft = &item.draft;
            println!(
                "  quote #{:<3}  {:<22}  {:<18}  next: {}",
                draft.id,
                draft.slug,
                draft.status.as_str(),
                item.next_action
            );
        }

        if dashboard.focus_items.is_empty() && dashboard.quote_items.is_empty() {
            println!("  (nothing active)");
        } else {
            for item in &dashboard.focus_items {
                let o = &item.order;
                let slug_or_title = o.slug.as_deref().unwrap_or(&o.title);
                let price = ui::format_price(o.final_price.or(o.quoted_price), &o.currency);
                let status_str = ui::display_status(o);

                let alert_part = if let Some(alert) = &item.alert {
                    format!("  {}", alert.yellow())
                } else {
                    String::new()
                };

                println!(
                    "  #{:<4}  {:<22}  {:<14}  {:<12}  next: {:<28}{}",
                    o.id, slug_or_title, status_str, price, item.next_action, alert_part
                );
            }
        }

        println!();
        let s = &dashboard.summary;
        println!(
            "  This month income: {}  |  Active: {}  |  Unpaid: {}",
            ui::format_price(Some(s.this_month_income), "CNY"),
            s.orders_count,
            s.pending_count,
        );
    }
    Ok(())
}

fn print_dashboard_json(dashboard: &gig_core::services::dashboard::Dashboard) {
    let output = json!({
        "status": "ok",
        "quote_items": dashboard.quote_items.iter().map(|item| {
            let draft = &item.draft;
            json!({
                "kind": "quote_draft",
                "id": draft.id,
                "slug": draft.slug,
                "title": draft.title,
                "status": draft.status.as_str(),
                "project_type": draft.project_type.as_str(),
                "next_action": item.next_action,
            })
        }).collect::<Vec<_>>(),
        "order_items": dashboard.focus_items.iter().map(|item| {
            let order = &item.order;
            let workflow = item.workflow.as_ref();
            json!({
                "kind": "order",
                "id": order.id,
                "slug": order.slug,
                "title": order.title,
                "status": order.status.as_str(),
                "next_action": item.next_action,
                "alert": item.alert,
                "project_type": workflow.and_then(|workflow| workflow.project_type.map(|project_type| project_type.as_str())),
            })
        }).collect::<Vec<_>>(),
        "summary": {
            "this_month_income": dashboard.summary.this_month_income,
            "orders_count": dashboard.summary.orders_count,
            "pending_count": dashboard.summary.pending_count,
        }
    });
    println!("{output}");
}
