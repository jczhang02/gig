use crate::cli::{AcceptanceArgs, AcceptanceCheckArgs, AcceptanceCommand, AcceptanceCompleteArgs};
use crate::commands::resolve_order;
use gig_core::services::workflow::{
    check_acceptance, complete_acceptance, AcceptanceCompleteInput, WorkflowResult,
};
use gig_core::Result;
use rusqlite::Connection;
use serde_json::json;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: AcceptanceArgs) -> Result<()> {
    match args.command {
        AcceptanceCommand::Check(args) => check(conn, args),
        AcceptanceCommand::Complete(args) => complete(conn, args),
    }
}

fn check(conn: &Connection, args: AcceptanceCheckArgs) -> Result<()> {
    let order = resolve_order(Some(args.id_or_slug), conn)?;
    let result = check_acceptance(conn, order.id)?;
    if args.json {
        print_workflow_json(&result, "complete_acceptance");
    } else {
        println!("acceptance checked for order #{}", result.order.id);
        println!("status: {}", result.order.status.as_str());
        println!("next_action: complete_acceptance");
    }
    Ok(())
}

fn complete(conn: &Connection, args: AcceptanceCompleteArgs) -> Result<()> {
    let order = resolve_order(Some(args.id_or_slug), conn)?;
    let completed_at = now_rfc3339();
    let result = complete_acceptance(
        conn,
        order.id,
        AcceptanceCompleteInput {
            completed_at: &completed_at,
        },
    )?;
    if args.json {
        print_workflow_json(&result, "send_package");
    } else {
        println!("acceptance complete for order #{}", result.order.id);
        println!("status: {}", result.order.status.as_str());
        println!("next_action: send_package");
    }
    Ok(())
}

fn print_workflow_json(result: &WorkflowResult, next_action: &str) {
    let workflow = &result.workflow;
    let output = json!({
        "status": result.order.status.as_str(),
        "next_action": next_action,
        "order": {
            "id": result.order.id,
            "slug": result.order.slug,
            "title": result.order.title,
            "status": result.order.status.as_str(),
            "project_type": result.order.project_type.map(|project_type| project_type.as_str()),
        },
        "paths": {
            "gig_dir": workflow.gig_dir,
            "acceptance_path": workflow.acceptance_path,
            "latest_delivery_dir": workflow.latest_delivery_dir,
            "latest_client_package_path": workflow.latest_client_package_path,
        },
        "workflow": {
            "acceptance_completed_at": workflow.acceptance_completed_at,
        }
    });
    println!("{output}");
}

fn now_rfc3339() -> String {
    let now = OffsetDateTime::now_utc();
    now.format(&Rfc3339)
        .unwrap_or_else(|_| now.unix_timestamp().to_string())
}
