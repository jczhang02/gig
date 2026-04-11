use crate::cli::NewArgs;
use crate::ui;
use gig_core::config::Config;
use gig_core::config::Paths;
use gig_core::services::orders::{create_order, CreateOrderInput};
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: NewArgs) -> Result<()> {
    // Load config for defaults. Safe to call from here — it's a config read, not logic.
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    let cut_ratio = args.cut_ratio.unwrap_or(config.general.default_cut_ratio);
    let currency = args
        .currency
        .unwrap_or(config.general.default_currency.clone());

    let input = CreateOrderInput {
        title: &args.title,
        slug: args.slug.as_deref(),
        client_id: None,
        source_org: args.source_org.as_deref(),
        quoted_price: args.quoted_price,
        final_price: args.final_price,
        my_cut_ratio: cut_ratio,
        currency: &currency,
        notes: args.notes.as_deref(),
        as_lead: args.lead,
    };
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let order = create_order(conn, &input, now)?;

    println!("created order #{}", order.id);
    println!("{}", ui::order_detail(&order));
    Ok(())
}
