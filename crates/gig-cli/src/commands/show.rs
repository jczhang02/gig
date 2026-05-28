use crate::cli::ShowArgs;
use crate::commands::resolve_order;
use crate::ui;
use gig_core::models::{Order, OrderWorkflow};
use gig_core::repo::{order_workflow, sources};
use gig_core::services::dashboard::order_next_action;
use gig_core::services::lifecycle::{get_price_history, get_requirement_changes, list_tags};
use gig_core::Result;
use rusqlite::Connection;
use serde_json::json;

pub fn run(conn: &Connection, args: ShowArgs) -> Result<()> {
    let order = resolve_order(args.id, conn)?;
    let tags = list_tags(conn, order.id)?;
    let price_history = get_price_history(conn, order.id)?;
    let req_changes = get_requirement_changes(conn, order.id)?;
    let workflow = order_workflow::find_by_order_id(conn, order.id)?;
    let next_action = order_next_action(&order, workflow.as_ref());

    if args.json {
        print_order_json(&order, workflow.as_ref(), next_action);
        return Ok(());
    }

    let source_label: Option<String> = order.source_id.and_then(|sid| {
        sources::find_by_id(conn, sid)
            .ok()
            .map(|source| format!("{} (cut {:.0}%)", source.name, source.cut_ratio * 100.0))
    });

    println!(
        "{}",
        ui::order_detail_full(
            &order,
            &tags,
            &price_history,
            &req_changes,
            source_label.as_deref()
        )
    );
    print_workflow_human(workflow.as_ref(), next_action);
    Ok(())
}

fn print_order_json(order: &Order, workflow: Option<&OrderWorkflow>, next_action: &str) {
    let output = json!({
        "status": "ok",
        "next_action": next_action,
        "order": {
            "id": order.id,
            "slug": order.slug.as_deref(),
            "title": order.title.as_str(),
            "status": order.status.as_str(),
            "project_type": order.project_type.map(|project_type| project_type.as_str()),
        },
        "workflow": {
            "project_type": workflow.and_then(|workflow| workflow.project_type.map(|project_type| project_type.as_str())),
            "paths": {
                "gig_dir": workflow.and_then(|workflow| workflow.gig_dir.as_deref()),
                "index_path": workflow.and_then(|workflow| workflow.index_path.as_deref()),
                "job_path": workflow.and_then(|workflow| workflow.job_path.as_deref()),
                "quote_path": workflow.and_then(|workflow| workflow.quote_path.as_deref()),
                "plan_md_path": workflow.and_then(|workflow| workflow.plan_md_path.as_deref()),
                "plan_html_path": workflow.and_then(|workflow| workflow.plan_html_path.as_deref()),
                "acceptance_path": workflow.and_then(|workflow| workflow.acceptance_path.as_deref()),
                "latest_delivery_dir": workflow.and_then(|workflow| workflow.latest_delivery_dir.as_deref()),
                "latest_client_package_path": workflow.and_then(|workflow| workflow.latest_client_package_path.as_deref()),
            },
            "plan_ready_at": workflow.and_then(|workflow| workflow.plan_ready_at.as_deref()),
            "plan_approved_at": workflow.and_then(|workflow| workflow.plan_approved_at.as_deref()),
            "plan_rejected_at": workflow.and_then(|workflow| workflow.plan_rejected_at.as_deref()),
            "plan_rejection_reason": workflow.and_then(|workflow| workflow.plan_rejection_reason.as_deref()),
            "acceptance_completed_at": workflow.and_then(|workflow| workflow.acceptance_completed_at.as_deref()),
        }
    });
    println!("{output}");
}

fn print_workflow_human(workflow: Option<&OrderWorkflow>, next_action: &str) {
    if let Some(workflow) = workflow {
        println!("workflow:");
        println!("  next_action  : {next_action}");
        println!(
            "  gig_dir      : {}",
            workflow.gig_dir.as_deref().unwrap_or("-")
        );
        println!(
            "  index_path   : {}",
            workflow.index_path.as_deref().unwrap_or("-")
        );
        println!(
            "  job_path     : {}",
            workflow.job_path.as_deref().unwrap_or("-")
        );
        println!(
            "  quote_path   : {}",
            workflow.quote_path.as_deref().unwrap_or("-")
        );
        println!(
            "  plan_md_path : {}",
            workflow.plan_md_path.as_deref().unwrap_or("-")
        );
        println!(
            "  plan_html_path: {}",
            workflow.plan_html_path.as_deref().unwrap_or("-")
        );
        println!(
            "  acceptance_path: {}",
            workflow.acceptance_path.as_deref().unwrap_or("-")
        );
    } else if next_action == "legacy_workflow_metadata_missing" {
        println!("workflow:");
        println!("  next_action  : {next_action}");
        println!("  metadata     : missing");
    }
}
