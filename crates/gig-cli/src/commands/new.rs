use crate::cli::NewArgs;
use crate::ui::{self, parse_yuan};
use gig_core::config::Config;
use gig_core::config::Paths;
use gig_core::repo::sources;
use gig_core::services::orders::{create_order, CreateOrderInput};
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: NewArgs) -> Result<()> {
    // Load config for defaults. Safe to call from here — it's a config read, not logic.
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    let currency = args
        .currency
        .unwrap_or(config.general.default_currency.clone());

    // Resolve source if --source was provided
    let (source_id, source_cut_ratio) = match &args.source {
        Some(name) => match sources::find_by_name(conn, name)? {
            Some(s) => {
                let ratio = s.cut_ratio;
                (Some(s.id), Some(ratio))
            }
            None => {
                eprintln!("warning: source {:?} not found; ignoring --source flag", name);
                (None, None)
            }
        },
        None => (None, None),
    };

    // --cut-ratio overrides source's cut_ratio; source's cut_ratio overrides config default
    let cut_ratio = args
        .cut_ratio
        .or(source_cut_ratio)
        .unwrap_or(config.general.default_cut_ratio);

    let quoted_price = args
        .quoted_price
        .as_deref()
        .map(parse_yuan)
        .transpose()?;
    let final_price = args
        .final_price
        .as_deref()
        .map(parse_yuan)
        .transpose()?;

    let input = CreateOrderInput {
        title: &args.title,
        slug: args.slug.as_deref(),
        client_id: None,
        source_org: args.source_org.as_deref(),
        source_id,
        quoted_price,
        final_price,
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
