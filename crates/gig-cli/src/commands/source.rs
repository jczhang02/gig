use crate::cli::{SourceAddArgs, SourceArgs, SourceCommand};
use comfy_table::{presets::UTF8_FULL, Cell, ContentArrangement, Table};
use gig_core::repo::sources;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: SourceArgs) -> Result<()> {
    match args.command {
        SourceCommand::Add(a) => cmd_add(conn, a),
        SourceCommand::Ls => cmd_ls(conn),
    }
}

fn cmd_add(conn: &Connection, args: SourceAddArgs) -> Result<()> {
    let source = sources::insert(conn, &args.name, args.cut_ratio, args.notes.as_deref())?;
    println!("created source #{}: {} (cut {:.0}%)", source.id, source.name, source.cut_ratio * 100.0);
    Ok(())
}

fn cmd_ls(conn: &Connection) -> Result<()> {
    let list = sources::list(conn)?;
    if list.is_empty() {
        println!("no sources yet — use `gig source add` to create one");
        return Ok(());
    }
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["id", "name", "cut %", "notes"]);
    for s in &list {
        t.add_row(vec![
            Cell::new(s.id),
            Cell::new(&s.name),
            Cell::new(format!("{:.0}%", s.cut_ratio * 100.0)),
            Cell::new(s.notes.as_deref().unwrap_or("—")),
        ]);
    }
    println!("{t}");
    Ok(())
}
