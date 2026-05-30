use crate::cli::{WorkArgs, WorkCommand, WorkStartArgs};
use crate::commands::resolve_order;
use gig_core::services::workflow::{start_work, WorkflowResult, WorkflowStartInput};
use gig_core::Result;
use rusqlite::Connection;
use serde_json::json;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: WorkArgs) -> Result<()> {
    match args.command {
        WorkCommand::Start(args) => start(conn, args),
    }
}

fn start(conn: &Connection, args: WorkStartArgs) -> Result<()> {
    let order = resolve_order(Some(args.id_or_slug), conn)?;
    let started_at = now_rfc3339();
    let result = start_work(
        conn,
        order.id,
        WorkflowStartInput {
            started_at: &started_at,
        },
    )?;

    if args.json {
        print_workflow_json(&result);
    } else {
        println!("work started for order #{}", result.order.id);
        println!("status: {}", result.order.status.as_str());
        println!("next_action: complete_acceptance");
    }
    Ok(())
}

fn print_workflow_json(result: &WorkflowResult) {
    let workflow = &result.workflow;
    let output = json!({
        "status": result.order.status.as_str(),
        "next_action": "complete_acceptance",
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
        },
        "workflow": {
            "plan_ready_at": workflow.plan_ready_at,
            "plan_approved_at": workflow.plan_approved_at,
            "plan_rejected_at": workflow.plan_rejected_at,
            "plan_rejection_reason": workflow.plan_rejection_reason,
            "work_started_at": workflow.work_started_at,
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
