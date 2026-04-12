use crate::cli::{ClientArgs, ClientCommand};
use gig_core::repo::clients;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: ClientArgs) -> Result<()> {
    match args.command {
        ClientCommand::Ls => cmd_ls(conn),
        ClientCommand::Show(a) => cmd_show(conn, a.id),
    }
}

fn cmd_ls(conn: &Connection) -> Result<()> {
    let all = clients::list(conn)?;
    if all.is_empty() {
        println!("no clients");
        return Ok(());
    }
    println!("{:<6}  {:<24}  {:<20}  orders", "id", "name", "org");
    println!("{}", "─".repeat(60));
    for c in &all {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM orders WHERE client_id = ?1",
                rusqlite::params![c.id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        println!(
            "{:<6}  {:<24}  {:<20}  {}",
            c.id,
            truncate(&c.display_name, 24),
            truncate(c.source_org.as_deref().unwrap_or("—"), 20),
            count,
        );
    }
    Ok(())
}

fn cmd_show(conn: &Connection, id: i64) -> Result<()> {
    let c = clients::find_by_id(conn, id)?;
    println!("Client #{}", c.id);
    println!("  name    : {}", c.display_name);
    println!("  wechat  : {}", c.wechat_contact.as_deref().unwrap_or("—"));
    println!("  org     : {}", c.source_org.as_deref().unwrap_or("—"));
    if let Some(ref n) = c.notes {
        println!("  notes   : {n}");
    }

    // List their orders.
    let mut stmt = conn.prepare(
        "SELECT id, slug, title, status, final_price, currency
         FROM orders WHERE client_id = ?1 ORDER BY id DESC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(gig_core::Error::Db)?;

    if rows.is_empty() {
        println!("\n  no orders");
    } else {
        println!("\n  orders:");
        for (oid, slug, title, status, price, currency) in &rows {
            let price_str = price
                .map(|p| format!("{} {:.2}", currency, p as f64 / 100.0))
                .unwrap_or_else(|| "—".into());
            println!(
                "    #{:<4}  {:<20}  {:<12}  {}  {}",
                oid,
                slug.as_deref().unwrap_or("—"),
                status,
                price_str,
                truncate(title, 30),
            );
        }
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut r: String = s.chars().take(max - 1).collect();
        r.push('…');
        r
    }
}
