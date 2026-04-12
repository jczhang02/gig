use crate::cli::ExportArgs;
use gig_core::repo::orders::{list, ListFilter};
use gig_core::Result;
use rusqlite::Connection;
use std::io::Write;

pub fn run(conn: &Connection, args: ExportArgs) -> Result<()> {
    let orders = list(conn, &ListFilter { status: None })?;

    let output: Box<dyn Write> = match &args.output {
        Some(path) => {
            let f = std::fs::File::create(path)
                .map_err(|e| gig_core::Error::PathUnavailable(path.clone(), e))?;
            Box::new(f)
        }
        None => Box::new(std::io::stdout()),
    };

    match args.format.as_str() {
        "csv" => export_csv(output, &orders),
        "json" => export_json(output, &orders),
        other => Err(gig_core::Error::Invalid(format!(
            "unknown format {other:?}; use csv or json"
        ))),
    }
}

fn export_csv(mut w: Box<dyn Write>, orders: &[gig_core::models::Order]) -> Result<()> {
    writeln!(
        w,
        "id,slug,title,client_id,status,quoted_price,final_price,my_cut_amount,currency,created_at,paid_at"
    )
    .map_err(gig_core::Error::Io)?;

    for o in orders {
        let cut = o.my_cut_amount().map(|v| v.to_string()).unwrap_or_default();
        writeln!(
            w,
            "{},{},{},{},{},{},{},{},{},{},{}",
            o.id,
            csv_field(o.slug.as_deref().unwrap_or("")),
            csv_field(&o.title),
            o.client_id.map(|v| v.to_string()).unwrap_or_default(),
            o.status.as_str(),
            o.quoted_price.map(|v| v.to_string()).unwrap_or_default(),
            o.final_price.map(|v| v.to_string()).unwrap_or_default(),
            cut,
            o.currency,
            o.created_at,
            o.paid_at.map(|v| v.to_string()).unwrap_or_default(),
        )
        .map_err(gig_core::Error::Io)?;
    }
    Ok(())
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn export_json(mut w: Box<dyn Write>, orders: &[gig_core::models::Order]) -> Result<()> {
    writeln!(w, "[").map_err(gig_core::Error::Io)?;
    for (i, o) in orders.iter().enumerate() {
        let comma = if i + 1 < orders.len() { "," } else { "" };
        let cut = o.my_cut_amount();
        writeln!(
            w,
            "  {{\
            \"id\":{},\"slug\":{},\"title\":{},\"client_id\":{},\
            \"status\":\"{}\",\"quoted_price\":{},\"final_price\":{},\
            \"my_cut_amount\":{},\"currency\":\"{}\",\
            \"created_at\":{},\"paid_at\":{}\
            }}{}",
            o.id,
            json_str(o.slug.as_deref()),
            json_str(Some(o.title.as_str())),
            json_opt_i64(o.client_id),
            o.status.as_str(),
            json_opt_i64(o.quoted_price),
            json_opt_i64(o.final_price),
            json_opt_i64(cut),
            o.currency,
            o.created_at,
            json_opt_i64(o.paid_at),
            comma,
        )
        .map_err(gig_core::Error::Io)?;
    }
    writeln!(w, "]").map_err(gig_core::Error::Io)?;
    Ok(())
}

fn json_str(s: Option<&str>) -> String {
    match s {
        None => "null".into(),
        Some(v) => format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\"")),
    }
}

fn json_opt_i64(v: Option<i64>) -> String {
    match v {
        None => "null".into(),
        Some(n) => n.to_string(),
    }
}
