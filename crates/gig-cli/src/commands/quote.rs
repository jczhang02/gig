use crate::cli::{
    QuoteAcceptArgs, QuoteArgs, QuoteCommand, QuoteDropArgs, QuoteListArgs, QuoteMarkSentArgs,
    QuoteNewArgs, QuotePriceArgs, QuoteShowArgs,
};
use crate::ui::parse_yuan;
use gig_core::config::Paths;
use gig_core::models::{ProjectType, QuoteDraft, QuoteDraftStatus};
use gig_core::repo::quote_drafts;
use gig_core::services::quote::{
    accept_quote_draft, drop_quote_draft, mark_quote_sent, price_quote_draft, record_quote_draft,
    QuoteAcceptInput, QuoteAcceptResult, QuoteDraftInput, QuotePriceInput,
};
use gig_core::{Error, Result};
use rusqlite::Connection;
use serde_json::json;
use std::str::FromStr;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: QuoteArgs) -> Result<()> {
    match args.command {
        QuoteCommand::New(args) => new_quote(conn, args),
        QuoteCommand::Show(args) => show_quote(conn, args),
        QuoteCommand::List(args) => list_quotes(conn, args),
        QuoteCommand::Price(args) => price_quote(conn, args),
        QuoteCommand::MarkSent(args) => mark_sent(conn, args),
        QuoteCommand::Accept(args) => accept_quote(conn, args),
        QuoteCommand::Drop(args) => drop_quote(conn, args),
    }
}

fn show_quote(conn: &Connection, args: QuoteShowArgs) -> Result<()> {
    let draft = resolve_quote_draft(conn, &args.id_or_slug)?;
    if args.json {
        print_quote_json(&draft);
    } else {
        print_quote_human(&draft);
    }
    Ok(())
}

fn list_quotes(conn: &Connection, args: QuoteListArgs) -> Result<()> {
    let drafts = quote_drafts::list(conn)?;
    if args.json {
        let output = json!({
            "status": "ok",
            "quote_drafts": drafts.iter().map(quote_json_value).collect::<Vec<_>>(),
        });
        println!("{output}");
    } else {
        for draft in drafts {
            println!("#{} {} [{}]", draft.id, draft.slug, draft.status.as_str());
        }
    }
    Ok(())
}

fn price_quote(conn: &Connection, args: QuotePriceArgs) -> Result<()> {
    let draft = resolve_quote_draft(conn, &args.id_or_slug)?;
    let now = now_rfc3339();
    let priced = price_quote_draft(
        conn,
        draft.id,
        QuotePriceInput {
            quote_min: parse_yuan(&args.min)?,
            quote_recommended: parse_yuan(&args.recommended)?,
            quote_max: parse_yuan(&args.max)?,
            updated_at: &now,
        },
    )?;
    if args.json {
        print_quote_json(&priced);
    } else {
        println!("priced quote draft #{}", priced.id);
        println!("status: {}", priced.status.as_str());
        println!("next_action: {}", next_action(priced.status));
    }
    Ok(())
}

fn mark_sent(conn: &Connection, args: QuoteMarkSentArgs) -> Result<()> {
    let draft = resolve_quote_draft(conn, &args.id_or_slug)?;
    let now = now_rfc3339();
    let sent = mark_quote_sent(conn, draft.id, &now)?;
    if args.json {
        print_quote_json(&sent);
    } else {
        println!("marked quote draft #{} as sent", sent.id);
        println!("status: {}", sent.status.as_str());
        println!("next_action: {}", next_action(sent.status));
    }
    Ok(())
}

fn accept_quote(conn: &Connection, args: QuoteAcceptArgs) -> Result<()> {
    let draft = resolve_quote_draft(conn, &args.id_or_slug)?;
    let now = OffsetDateTime::now_utc();
    let accepted_at = format_rfc3339(now);
    let result = accept_quote_draft(
        conn,
        draft.id,
        QuoteAcceptInput {
            project_dir: &args.project_dir,
            accepted_at: &accepted_at,
            accepted_at_unix: now.unix_timestamp(),
            my_cut_ratio: args.my_cut_ratio,
        },
    )?;

    if args.json {
        print_accept_json(&result);
    } else {
        println!("accepted quote draft #{}", result.quote_draft.id);
        println!("order: #{}", result.order.id);
        println!("status: {}", result.quote_draft.status.as_str());
        println!("next_action: prepare_plan");
    }
    Ok(())
}

fn drop_quote(conn: &Connection, args: QuoteDropArgs) -> Result<()> {
    let draft = resolve_quote_draft(conn, &args.id_or_slug)?;
    let now = now_rfc3339();
    let dropped = drop_quote_draft(conn, draft.id, &args.reason, &now)?;
    if args.json {
        print_quote_json(&dropped);
    } else {
        println!("dropped quote draft #{}", dropped.id);
        println!("status: {}", dropped.status.as_str());
        println!("next_action: none");
    }
    Ok(())
}

fn new_quote(conn: &Connection, args: QuoteNewArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let project_type = ProjectType::from_str(&args.project_type)?;
    let now = now_rfc3339();
    let draft = record_quote_draft(
        conn,
        &paths,
        &QuoteDraftInput {
            slug: &args.slug,
            title: &args.title,
            client_label: args.client_label.as_deref(),
            source_org: args.source_org.as_deref(),
            project_type,
            summary: &args.summary,
            created_at: &now,
        },
    )?;

    if args.json {
        print_quote_new_json(&draft);
    } else {
        println!("recorded quote draft #{}", draft.id);
        println!("status: {}", draft.status.as_str());
        println!("next_action: price_quote");
        println!("xdg_path: {}", draft.xdg_path);
    }

    Ok(())
}

fn print_quote_new_json(draft: &QuoteDraft) {
    print_quote_json(draft);
}

fn print_quote_json(draft: &QuoteDraft) {
    let output = json!({
        "status": draft.status.as_str(),
        "next_action": next_action(draft.status),
        "quote_draft": quote_json_value(draft),
        "paths": {
            "xdg_path": draft.xdg_path,
        }
    });
    println!("{output}");
}

fn print_accept_json(result: &QuoteAcceptResult) {
    let workflow = &result.workflow;
    let output = json!({
        "status": result.quote_draft.status.as_str(),
        "next_action": "prepare_plan",
        "quote_draft": quote_json_value(&result.quote_draft),
        "order": {
            "id": result.order.id,
            "slug": result.order.slug,
            "title": result.order.title,
            "status": result.order.status.as_str(),
            "project_type": result.order.project_type.map(|project_type| project_type.as_str()),
        },
        "paths": {
            "gig_dir": workflow.gig_dir,
            "index_path": workflow.index_path,
            "job_path": workflow.job_path,
            "quote_path": workflow.quote_path,
            "plan_md_path": workflow.plan_md_path,
            "plan_html_path": workflow.plan_html_path,
            "acceptance_path": workflow.acceptance_path,
        }
    });
    println!("{output}");
}

fn quote_json_value(draft: &QuoteDraft) -> serde_json::Value {
    json!({
        "id": draft.id,
        "slug": draft.slug,
        "title": draft.title,
        "project_type": draft.project_type.as_str(),
        "client_label": draft.client_label,
        "source_org": draft.source_org,
        "summary": draft.summary,
        "status": draft.status.as_str(),
        "quote_min": draft.quote_min,
        "quote_recommended": draft.quote_recommended,
        "quote_max": draft.quote_max,
        "currency": draft.currency,
        "drop_reason": draft.drop_reason,
    })
}

fn print_quote_human(draft: &QuoteDraft) {
    println!("quote draft #{}", draft.id);
    println!("slug: {}", draft.slug);
    println!("title: {}", draft.title);
    println!("status: {}", draft.status.as_str());
    println!("next_action: {}", next_action(draft.status));
    println!("xdg_path: {}", draft.xdg_path);
}

fn resolve_quote_draft(conn: &Connection, id_or_slug: &str) -> Result<QuoteDraft> {
    if let Ok(id) = id_or_slug.parse::<i64>() {
        return quote_drafts::find_by_id(conn, id);
    }

    quote_drafts::find_by_slug(conn, id_or_slug)?
        .ok_or_else(|| Error::Invalid(format!("quote draft not found: {id_or_slug}")))
}

fn next_action(status: QuoteDraftStatus) -> &'static str {
    match status {
        QuoteDraftStatus::QuoteDraft | QuoteDraftStatus::NeedsClarification => "price_quote",
        QuoteDraftStatus::Quoted => "send_or_accept",
        QuoteDraftStatus::Accepted | QuoteDraftStatus::Dropped => "none",
    }
}

fn now_rfc3339() -> String {
    format_rfc3339(OffsetDateTime::now_utc())
}

fn format_rfc3339(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&Rfc3339)
        .unwrap_or_else(|_| timestamp.unix_timestamp().to_string())
}
