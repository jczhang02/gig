use crate::cli::{PlanApproveArgs, PlanArgs, PlanCommand, PlanReadyArgs, PlanRejectArgs};
use crate::commands::resolve_order;
use gig_core::services::workflow::{
    approve_plan, mark_plan_ready, reject_plan, PlanApprovalInput, PlanReadyInput,
    PlanRejectionInput, WorkflowResult,
};
use gig_core::Result;
use rusqlite::Connection;
use serde_json::json;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: PlanArgs) -> Result<()> {
    match args.command {
        PlanCommand::Ready(args) => ready(conn, args),
        PlanCommand::Approve(args) => approve(conn, args),
        PlanCommand::Reject(args) => reject(conn, args),
    }
}

fn ready(conn: &Connection, args: PlanReadyArgs) -> Result<()> {
    let order = resolve_order(Some(args.id_or_slug), conn)?;
    let ready_at = now_rfc3339();
    let result = mark_plan_ready(
        conn,
        order.id,
        PlanReadyInput {
            ready_at: &ready_at,
        },
    )?;
    if args.json {
        print_workflow_json(&result, "approve_plan");
    } else {
        println!("plan ready for order #{}", result.order.id);
        println!("status: {}", result.order.status.as_str());
        println!("next_action: approve_plan");
    }
    Ok(())
}

fn approve(conn: &Connection, args: PlanApproveArgs) -> Result<()> {
    let order = resolve_order(Some(args.id_or_slug), conn)?;
    let approved_at = now_rfc3339();
    let result = approve_plan(
        conn,
        order.id,
        PlanApprovalInput {
            approved_at: &approved_at,
        },
    )?;
    if args.json {
        print_workflow_json(&result, "start_work");
    } else {
        println!("plan approved for order #{}", result.order.id);
        println!("status: {}", result.order.status.as_str());
        println!("next_action: start_work");
    }
    Ok(())
}

fn reject(conn: &Connection, args: PlanRejectArgs) -> Result<()> {
    let order = resolve_order(Some(args.id_or_slug), conn)?;
    let rejected_at = now_rfc3339();
    let result = reject_plan(
        conn,
        order.id,
        PlanRejectionInput {
            rejected_at: &rejected_at,
            reason: &args.reason,
        },
    )?;
    if args.json {
        print_workflow_json(&result, "revise_plan");
    } else {
        println!("plan rejected for order #{}", result.order.id);
        println!("status: {}", result.order.status.as_str());
        println!("next_action: revise_plan");
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
