use crate::cli::ExportArgs;
use gig_core::repo::orders::{list, ListFilter};
use gig_core::Result;
use rusqlite::Connection;
use std::io::Write;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

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

fn unix_to_iso(ts: i64) -> String {
    OffsetDateTime::from_unix_timestamp(ts)
        .ok()
        .and_then(|dt| dt.format(&Rfc3339).ok())
        .unwrap_or_else(|| ts.to_string())
}

fn unix_to_iso_opt(ts: Option<i64>) -> String {
    match ts {
        None => String::new(),
        Some(t) => unix_to_iso(t),
    }
}

fn cents_to_yuan(cents: Option<i64>) -> String {
    match cents {
        None => String::new(),
        Some(c) => format!("{:.2}", c as f64 / 100.0),
    }
}

fn export_csv(mut w: Box<dyn Write>, orders: &[gig_core::models::Order]) -> Result<()> {
    writeln!(
        w,
        "id,slug,title,client_id,status,quoted_price,final_price,my_cut_amount,currency,created_at,paid_at"
    )
    .map_err(gig_core::Error::Io)?;

    for o in orders {
        let cut = cents_to_yuan(o.my_cut_amount());
        writeln!(
            w,
            "{},{},{},{},{},{},{},{},{},{},{}",
            o.id,
            csv_field(o.slug.as_deref().unwrap_or("")),
            csv_field(&o.title),
            o.client_id.map(|v| v.to_string()).unwrap_or_default(),
            o.status.as_str(),
            cents_to_yuan(o.quoted_price),
            cents_to_yuan(o.final_price),
            cut,
            o.currency,
            unix_to_iso(o.created_at),
            unix_to_iso_opt(o.paid_at),
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

fn json_opt_yuan(v: Option<i64>) -> String {
    match v {
        None => "null".into(),
        Some(n) => format!("{:.2}", n as f64 / 100.0),
    }
}

fn export_json(mut w: Box<dyn Write>, orders: &[gig_core::models::Order]) -> Result<()> {
    writeln!(w, "[").map_err(gig_core::Error::Io)?;
    for (i, o) in orders.iter().enumerate() {
        let comma = if i + 1 < orders.len() { "," } else { "" };
        let cut = o.my_cut_amount();
        let created_iso = unix_to_iso(o.created_at);
        let paid_iso = o.paid_at.map(unix_to_iso);
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
            json_opt_yuan(o.quoted_price),
            json_opt_yuan(o.final_price),
            json_opt_yuan(cut),
            o.currency,
            json_str(Some(&created_iso)),
            json_str(paid_iso.as_deref()),
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
