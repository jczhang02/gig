//! Output formatting helpers.

use comfy_table::{presets::UTF8_FULL, Cell, ContentArrangement, Table};
use gig_core::models::{Order, OrderStatus, PriceHistoryEntry, RequirementChange, Tag};
use gig_core::{Error, Result};
use owo_colors::OwoColorize;
use std::io::{self, BufRead, Write};

/// Parse a user-input price string in yuan to minor units (cents).
/// "1200" → 120000, "1200.50" → 120050, "1200.5" → 120050
pub fn parse_yuan(s: &str) -> Result<i64> {
    let s = s.trim();
    if let Some((integer_part, frac_part)) = s.split_once('.') {
        let integer: i64 = integer_part
            .parse()
            .map_err(|_| Error::Invalid(format!("invalid price: {s}")))?;
        // Pad or truncate fractional part to exactly 2 digits
        let frac_str = format!("{:0<2}", &frac_part[..frac_part.len().min(2)]);
        let frac: i64 = frac_str
            .parse()
            .map_err(|_| Error::Invalid(format!("invalid price: {s}")))?;
        Ok(integer * 100 + frac)
    } else {
        let integer: i64 = s
            .parse()
            .map_err(|_| Error::Invalid(format!("invalid price: {s}")))?;
        Ok(integer * 100)
    }
}

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
    order_detail_full(o, &[], &[], &[], None)
}

pub fn order_detail_full(
    o: &Order,
    tags: &[Tag],
    price_history: &[PriceHistoryEntry],
    req_changes: &[RequirementChange],
    source_display: Option<&str>,
) -> String {
    let mut s = String::new();
    s.push_str(&format!("Order #{}  {}\n", o.id, o.title.bold()));
    s.push_str(&format!(
        "  slug         : {}\n",
        o.slug.as_deref().unwrap_or("—")
    ));
    s.push_str(&format!("  status       : {}\n", colour_status(o.status)));
    s.push_str(&format!(
        "  quoted       : {}\n",
        format_price(o.quoted_price, &o.currency)
    ));
    s.push_str(&format!(
        "  final        : {}\n",
        format_price(o.final_price, &o.currency)
    ));
    s.push_str(&format!("  cut ratio    : {:.2}\n", o.my_cut_ratio));
    s.push_str(&format!(
        "  cut amount   : {}\n",
        format_price(o.my_cut_amount(), &o.currency)
    ));
    s.push_str(&format!("  currency     : {}\n", o.currency));
    s.push_str(&format!(
        "  source_org   : {}\n",
        o.source_org.as_deref().unwrap_or("—")
    ));
    if let Some(src) = source_display {
        s.push_str(&format!("  source       : {src}\n"));
    }
    s.push_str(&format!(
        "  dev_path     : {}\n",
        o.dev_path.as_deref().unwrap_or("—")
    ));
    s.push_str(&format!(
        "  archive_path : {}\n",
        o.archive_path.as_deref().unwrap_or("—")
    ));
    if let Some(n) = &o.notes {
        s.push_str(&format!("  notes        : {n}\n"));
    }

    // Tags
    if !tags.is_empty() {
        let tag_names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
        s.push_str(&format!("  tags         : {}\n", tag_names.join(", ")));
    }

    // Price history
    if !price_history.is_empty() {
        s.push_str("  price history:\n");
        for entry in price_history {
            s.push_str(&format!(
                "    #{}: {} → {}",
                entry.id,
                format_price(entry.old_price, &o.currency),
                format_price(entry.new_price, &o.currency),
            ));
            if let Some(ref r) = entry.reason {
                s.push_str(&format!("  [{r}]"));
            }
            s.push('\n');
        }
    }

    // Requirement changes
    if !req_changes.is_empty() {
        s.push_str("  req changes  :\n");
        for rc in req_changes {
            let delta_str = if rc.price_delta >= 0 {
                format!("+{}", format_price(Some(rc.price_delta), &o.currency))
            } else {
                format!("-{}", format_price(Some(-rc.price_delta), &o.currency))
            };
            s.push_str(&format!(
                "    #{}: {}  ({})\n",
                rc.id, rc.description, delta_str
            ));
        }
    }

    s
}

/// Print a one-line banner identifying the order being operated on.
pub fn print_banner(o: &Order) {
    let price = format_price(o.final_price.or(o.quoted_price), &o.currency);
    eprintln!(
        "\u{2192} order #{}  {}  ({}, {})",
        o.id,
        o.slug.as_deref().unwrap_or(&o.title),
        colour_status(o.status),
        price,
    );
}

/// Prompt the user for y/N confirmation. Returns true if user typed 'y' or 'Y'.
pub fn confirm(prompt: &str) -> bool {
    eprint!("{prompt} [y/N] ");
    io::stderr().flush().ok();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim(), "y" | "Y")
}
