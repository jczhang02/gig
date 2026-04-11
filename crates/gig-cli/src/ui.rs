//! Output formatting helpers.

use comfy_table::{presets::UTF8_FULL, Cell, ContentArrangement, Table};
use gig_core::models::{Order, OrderStatus};
use owo_colors::OwoColorize;

pub fn format_price(minor: Option<i64>, currency: &str) -> String {
    match minor {
        None => "—".into(),
        Some(m) => {
            let major = m as f64 / 100.0;
            format!("{currency} {major:.2}")
        }
    }
}

pub fn colour_status(status: OrderStatus) -> String {
    let s = status.as_str();
    match status {
        OrderStatus::Lead => s.yellow().to_string(),
        OrderStatus::Negotiating => s.yellow().to_string(),
        OrderStatus::Accepted => s.cyan().to_string(),
        OrderStatus::InProgress => s.blue().to_string(),
        OrderStatus::Delivered => s.green().to_string(),
        OrderStatus::Paid => s.green().bold().to_string(),
        OrderStatus::Archived => s.bright_black().to_string(),
        OrderStatus::Cancelled => s.red().to_string(),
    }
}

pub fn orders_table(orders: &[Order]) -> Table {
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["id", "slug", "title", "status", "quoted", "final"]);
    for o in orders {
        t.add_row(vec![
            Cell::new(o.id),
            Cell::new(o.slug.as_deref().unwrap_or("—")),
            Cell::new(&o.title),
            Cell::new(colour_status(o.status)),
            Cell::new(format_price(o.quoted_price, &o.currency)),
            Cell::new(format_price(o.final_price, &o.currency)),
        ]);
    }
    t
}

pub fn order_detail(o: &Order) -> String {
    let mut s = String::new();
    s.push_str(&format!("Order #{}  {}\n", o.id, o.title.bold()));
    s.push_str(&format!("  slug         : {}\n", o.slug.as_deref().unwrap_or("—")));
    s.push_str(&format!("  status       : {}\n", colour_status(o.status)));
    s.push_str(&format!("  quoted       : {}\n", format_price(o.quoted_price, &o.currency)));
    s.push_str(&format!("  final        : {}\n", format_price(o.final_price, &o.currency)));
    s.push_str(&format!("  cut ratio    : {:.2}\n", o.my_cut_ratio));
    s.push_str(&format!("  cut amount   : {}\n", format_price(o.my_cut_amount(), &o.currency)));
    s.push_str(&format!("  currency     : {}\n", o.currency));
    s.push_str(&format!("  source_org   : {}\n", o.source_org.as_deref().unwrap_or("—")));
    s.push_str(&format!("  dev_path     : {}\n", o.dev_path.as_deref().unwrap_or("—")));
    s.push_str(&format!("  archive_path : {}\n", o.archive_path.as_deref().unwrap_or("—")));
    if let Some(n) = &o.notes {
        s.push_str(&format!("  notes        : {n}\n"));
    }
    s
}
