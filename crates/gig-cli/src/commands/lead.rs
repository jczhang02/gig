use crate::cli::{LeadArgs, LeadCommand};
use crate::ui;
use gig_core::models::OrderStatus;
use gig_core::repo::orders::{find_by_id_or_slug, list, ListFilter};
use gig_core::services::lifecycle::{drop_lead, promote_lead};
use gig_core::Result;
use rusqlite::Connection;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: LeadArgs) -> Result<()> {
    match args.command {
        LeadCommand::Promote(a) => {
            let order = find_by_id_or_slug(conn, &a.id)?;
            let now = OffsetDateTime::now_utc().unix_timestamp();
            let updated = promote_lead(conn, order.id, now)?;
            println!("lead #{} promoted to negotiating", updated.id);
            println!("{}", ui::order_detail(&updated));
        }
        LeadCommand::Drop(a) => {
            let order = find_by_id_or_slug(conn, &a.id)?;
            let now = OffsetDateTime::now_utc().unix_timestamp();
            let updated = drop_lead(conn, order.id, now)?;
            println!("lead #{} dropped (cancelled)", updated.id);
            println!("{}", ui::order_detail(&updated));
        }
        LeadCommand::Ls => {
            let orders = list(
                conn,
                &ListFilter {
                    status: Some(OrderStatus::Lead),
                },
            )?;
            if orders.is_empty() {
                println!("no leads");
            } else {
                let table = ui::orders_table(&orders);
                println!("{table}");
            }
        }
    }
    Ok(())
}
