use crate::models::{Order, OrderStatus, OrderWorkflow};
use crate::repo::{order_workflow, orders};
use crate::{Error, Result};
use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct PlanReadyInput<'a> {
    pub ready_at: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct PlanApprovalInput<'a> {
    pub approved_at: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct PlanRejectionInput<'a> {
    pub rejected_at: &'a str,
    pub reason: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct AcceptanceCompleteInput<'a> {
    pub completed_at: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct WorkflowStartInput<'a> {
    pub started_at: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowResult {
    pub order: Order,
    pub workflow: OrderWorkflow,
}

pub fn mark_plan_ready(
    conn: &Connection,
    order_id: i64,
    input: PlanReadyInput<'_>,
) -> Result<WorkflowResult> {
    let order = orders::find_by_id(conn, order_id)?;
    if order.status != OrderStatus::Accepted {
        return Err(Error::Invalid(format!(
            "cannot mark plan ready from {}",
            order.status
        )));
    }

    let workflow = order_workflow::find_by_order_id(conn, order_id)?
        .ok_or_else(|| Error::Invalid(format!("missing order_workflow for order {order_id}")))?;
    let plan_md_path = workflow
        .plan_md_path
        .as_deref()
        .ok_or_else(|| Error::Invalid("missing expected plan markdown path".to_string()))?;
    let plan_html_path = workflow
        .plan_html_path
        .as_deref()
        .ok_or_else(|| Error::Invalid("missing expected plan html path".to_string()))?;

    require_workflow_created_file(Path::new(plan_md_path), "plan markdown")?;
    require_workflow_created_file(Path::new(plan_html_path), "plan html")?;

    orders::update_status(conn, order_id, OrderStatus::PlanReady, None, None)?;
    let order = orders::find_by_id(conn, order_id)?;
    let workflow = order_workflow::record_plan_ready(conn, order_id, input.ready_at)?;
    Ok(WorkflowResult { order, workflow })
}

pub fn approve_plan(
    conn: &Connection,
    order_id: i64,
    input: PlanApprovalInput<'_>,
) -> Result<WorkflowResult> {
    require_order_status(conn, order_id, OrderStatus::PlanReady)?;
    orders::update_status(conn, order_id, OrderStatus::PlanApproved, None, None)?;
    let order = orders::find_by_id(conn, order_id)?;
    let workflow = order_workflow::record_plan_approval(conn, order_id, input.approved_at)?;
    Ok(WorkflowResult { order, workflow })
}

pub fn reject_plan(
    conn: &Connection,
    order_id: i64,
    input: PlanRejectionInput<'_>,
) -> Result<WorkflowResult> {
    let reason = input.reason.trim();
    if reason.is_empty() {
        return Err(Error::Invalid(
            "plan rejection reason must not be empty".to_string(),
        ));
    }

    require_order_status(conn, order_id, OrderStatus::PlanReady)?;
    let workflow =
        order_workflow::record_plan_rejection(conn, order_id, reason, input.rejected_at)?;
    orders::update_status(conn, order_id, OrderStatus::Accepted, None, None)?;
    let order = orders::find_by_id(conn, order_id)?;
    Ok(WorkflowResult { order, workflow })
}

pub fn start_work(
    conn: &Connection,
    order_id: i64,
    input: WorkflowStartInput<'_>,
) -> Result<WorkflowResult> {
    require_order_status(conn, order_id, OrderStatus::PlanApproved)?;
    orders::update_status(conn, order_id, OrderStatus::InProgress, None, None)?;
    let order = orders::find_by_id(conn, order_id)?;
    let workflow = order_workflow::record_work_started(conn, order_id, input.started_at)?;
    Ok(WorkflowResult { order, workflow })
}

fn require_order_status(conn: &Connection, order_id: i64, expected: OrderStatus) -> Result<Order> {
    let order = orders::find_by_id(conn, order_id)?;
    if order.status == expected {
        Ok(order)
    } else {
        Err(Error::Invalid(format!(
            "expected order {order_id} to be {expected}, got {}",
            order.status
        )))
    }
}

pub fn check_acceptance(conn: &Connection, order_id: i64) -> Result<WorkflowResult> {
    let order = require_order_status(conn, order_id, OrderStatus::InProgress)?;
    let workflow = order_workflow::find_by_order_id(conn, order_id)?
        .ok_or_else(|| Error::Invalid(format!("missing order_workflow for order {order_id}")))?;
    let acceptance_path = workflow
        .acceptance_path
        .as_deref()
        .ok_or_else(|| Error::Invalid("missing expected acceptance path".to_string()))?;

    let path = Path::new(acceptance_path);
    require_workflow_created_file(path, "acceptance")?;
    let contents = std::fs::read_to_string(path)?;
    require_acceptance_headings(&contents)?;

    Ok(WorkflowResult { order, workflow })
}

pub fn complete_acceptance(
    conn: &Connection,
    order_id: i64,
    input: AcceptanceCompleteInput<'_>,
) -> Result<WorkflowResult> {
    check_acceptance(conn, order_id)?;
    let workflow = order_workflow::record_acceptance_completed(conn, order_id, input.completed_at)?;
    orders::update_status(conn, order_id, OrderStatus::ReadyToDeliver, None, None)?;
    let order = orders::find_by_id(conn, order_id)?;
    Ok(WorkflowResult { order, workflow })
}

fn require_acceptance_headings(contents: &str) -> Result<()> {
    let lower = contents.to_ascii_lowercase();
    let has_chinese = ["验收项", "方法", "证据", "结论"]
        .iter()
        .all(|heading| contents.contains(heading));
    let has_english = ["item", "method", "evidence", "conclusion"]
        .iter()
        .all(|heading| lower.contains(heading));

    if has_chinese || has_english {
        Ok(())
    } else {
        Err(Error::Invalid(
            "acceptance headings missing: require 验收项/方法/证据/结论 or English item/method/evidence/conclusion"
                .to_string(),
        ))
    }
}

fn require_workflow_created_file(path: &Path, label: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "missing workflow-created {label}: {}",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::{OrderStatus, ProjectType};
    use crate::repo::order_workflow::{self, NewOrderWorkflow};
    use crate::repo::orders::{self, NewOrder};

    fn accepted_order(conn: &rusqlite::Connection) -> i64 {
        orders::insert(
            conn,
            &NewOrder {
                slug: Some("workflow-order"),
                external_id: None,
                title: "Workflow order",
                client_id: None,
                source_org: None,
                source_id: None,
                project_type: Some(ProjectType::Crawler),
                status: OrderStatus::Accepted,
                quoted_price: Some(15_000),
                final_price: None,
                my_cut_ratio: 0.6,
                currency: "CNY",
                notes: None,
                created_at: 1_700_000_000,
                accepted_at: Some(1_700_000_000),
            },
        )
        .unwrap()
        .id
    }

    fn workflow_row(conn: &rusqlite::Connection, order_id: i64, gig_dir: &std::path::Path) {
        order_workflow::insert(
            conn,
            &NewOrderWorkflow {
                order_id,
                project_type: Some(ProjectType::Crawler),
                gig_dir: Some(gig_dir.to_str().unwrap()),
                index_path: Some(gig_dir.join("INDEX.html").to_str().unwrap()),
                job_path: Some(gig_dir.join("JOB.md").to_str().unwrap()),
                quote_path: Some(gig_dir.join("QUOTE.md").to_str().unwrap()),
                plan_md_path: Some(gig_dir.join("plan/PLAN.md").to_str().unwrap()),
                plan_html_path: Some(gig_dir.join("plan/PLAN.html").to_str().unwrap()),
                acceptance_path: Some(gig_dir.join("acceptance/ACCEPTANCE.md").to_str().unwrap()),
                created_at: "2026-05-27T02:00:00Z",
                updated_at: "2026-05-27T02:00:00Z",
            },
        )
        .unwrap();
    }

    #[test]
    fn mark_plan_ready_records_timestamp_and_sets_order_status_without_creating_files() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = accepted_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        let plan_dir = gig_dir.join("plan");
        std::fs::create_dir_all(&plan_dir).unwrap();
        std::fs::write(plan_dir.join("PLAN.md"), "workflow-created plan").unwrap();
        std::fs::write(plan_dir.join("PLAN.html"), "workflow-created html").unwrap();
        workflow_row(&conn, order_id, &gig_dir);

        let ready = mark_plan_ready(
            &conn,
            order_id,
            PlanReadyInput {
                ready_at: "2026-05-27T03:00:00Z",
            },
        )
        .unwrap();

        assert_eq!(ready.order.status, OrderStatus::PlanReady);
        assert_eq!(
            ready.workflow.plan_ready_at.as_deref(),
            Some("2026-05-27T03:00:00Z")
        );
        assert!(plan_dir.join("PLAN.md").exists());
        assert!(plan_dir.join("PLAN.html").exists());
        assert!(!gig_dir.join("acceptance/ACCEPTANCE.md").exists());
    }

    #[test]
    fn mark_plan_ready_requires_workflow_created_plan_files() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = accepted_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        workflow_row(&conn, order_id, &gig_dir);

        let err = mark_plan_ready(
            &conn,
            order_id,
            PlanReadyInput {
                ready_at: "2026-05-27T03:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("workflow-created plan"));
        let order = orders::find_by_id(&conn, order_id).unwrap();
        let workflow = order_workflow::find_by_order_id(&conn, order_id)
            .unwrap()
            .unwrap();
        assert_eq!(order.status, OrderStatus::Accepted);
        assert_eq!(workflow.plan_ready_at, None);
        assert!(!gig_dir.join("plan").exists());
    }

    fn ready_order(
        root: &tempfile::TempDir,
        conn: &rusqlite::Connection,
    ) -> (i64, std::path::PathBuf) {
        let order_id = accepted_order(conn);
        let gig_dir = root.path().join("project/.gig");
        let plan_dir = gig_dir.join("plan");
        std::fs::create_dir_all(&plan_dir).unwrap();
        std::fs::write(plan_dir.join("PLAN.md"), "workflow-created plan").unwrap();
        std::fs::write(plan_dir.join("PLAN.html"), "workflow-created html").unwrap();
        workflow_row(conn, order_id, &gig_dir);
        mark_plan_ready(
            conn,
            order_id,
            PlanReadyInput {
                ready_at: "2026-05-27T03:00:00Z",
            },
        )
        .unwrap();
        (order_id, gig_dir)
    }

    #[test]
    fn approve_plan_records_timestamp_and_sets_order_status_without_creating_files() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let (order_id, gig_dir) = ready_order(&root, &conn);

        let approved = approve_plan(
            &conn,
            order_id,
            PlanApprovalInput {
                approved_at: "2026-05-27T04:00:00Z",
            },
        )
        .unwrap();

        assert_eq!(approved.order.status, OrderStatus::PlanApproved);
        assert_eq!(
            approved.workflow.plan_approved_at.as_deref(),
            Some("2026-05-27T04:00:00Z")
        );
        assert!(gig_dir.join("plan/PLAN.md").exists());
        assert!(gig_dir.join("plan/PLAN.html").exists());
        assert!(!gig_dir.join("acceptance/ACCEPTANCE.md").exists());
    }

    #[test]
    fn reject_plan_records_reason_and_returns_order_to_accepted_without_rewriting_files() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let (order_id, gig_dir) = ready_order(&root, &conn);
        let plan_path = gig_dir.join("plan/PLAN.md");
        let before = std::fs::read_to_string(&plan_path).unwrap();

        let rejected = reject_plan(
            &conn,
            order_id,
            PlanRejectionInput {
                rejected_at: "2026-05-27T04:30:00Z",
                reason: "scope missing",
            },
        )
        .unwrap();

        assert_eq!(rejected.order.status, OrderStatus::Accepted);
        assert_eq!(
            rejected.workflow.plan_rejected_at.as_deref(),
            Some("2026-05-27T04:30:00Z")
        );
        assert_eq!(
            rejected.workflow.plan_rejection_reason.as_deref(),
            Some("scope missing")
        );
        assert_eq!(std::fs::read_to_string(&plan_path).unwrap(), before);
        assert!(gig_dir.join("plan/PLAN.html").exists());
    }

    #[test]
    fn reject_plan_requires_non_empty_reason() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let (order_id, _gig_dir) = ready_order(&root, &conn);

        let err = reject_plan(
            &conn,
            order_id,
            PlanRejectionInput {
                rejected_at: "2026-05-27T04:30:00Z",
                reason: "   ",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("rejection reason"));
        let order = orders::find_by_id(&conn, order_id).unwrap();
        let workflow = order_workflow::find_by_order_id(&conn, order_id)
            .unwrap()
            .unwrap();
        assert_eq!(order.status, OrderStatus::PlanReady);
        assert_eq!(workflow.plan_rejected_at, None);
        assert_eq!(workflow.plan_rejection_reason, None);
    }

    fn approved_order(
        root: &tempfile::TempDir,
        conn: &rusqlite::Connection,
    ) -> (i64, std::path::PathBuf) {
        let (order_id, gig_dir) = ready_order(root, conn);
        approve_plan(
            conn,
            order_id,
            PlanApprovalInput {
                approved_at: "2026-05-27T04:00:00Z",
            },
        )
        .unwrap();
        (order_id, gig_dir)
    }

    #[test]
    fn start_work_records_in_progress_only_after_plan_approval_without_creating_files() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let (order_id, gig_dir) = approved_order(&root, &conn);

        let started = start_work(
            &conn,
            order_id,
            WorkflowStartInput {
                started_at: "2026-05-27T04:45:00Z",
            },
        )
        .unwrap();

        assert_eq!(started.order.status, OrderStatus::InProgress);
        assert_eq!(started.workflow.updated_at, "2026-05-27T04:45:00Z");
        assert!(gig_dir.join("plan/PLAN.md").exists());
        assert!(gig_dir.join("plan/PLAN.html").exists());
        assert!(!gig_dir.join("acceptance/ACCEPTANCE.md").exists());
    }

    #[test]
    fn start_work_rejects_accepted_order_that_bypasses_plan_approval() {
        let conn = open_in_memory().unwrap();
        let order_id = accepted_order(&conn);

        let err = start_work(
            &conn,
            order_id,
            WorkflowStartInput {
                started_at: "2026-05-27T04:45:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("plan_approved"));
        let order = orders::find_by_id(&conn, order_id).unwrap();
        assert_eq!(order.status, OrderStatus::Accepted);
    }

    fn in_progress_order(conn: &rusqlite::Connection) -> i64 {
        orders::insert(
            conn,
            &NewOrder {
                slug: Some("in-progress-order"),
                external_id: None,
                title: "In progress order",
                client_id: None,
                source_org: None,
                source_id: None,
                project_type: Some(ProjectType::Crawler),
                status: OrderStatus::InProgress,
                quoted_price: Some(15_000),
                final_price: None,
                my_cut_ratio: 0.6,
                currency: "CNY",
                notes: None,
                created_at: 1_700_000_000,
                accepted_at: Some(1_700_000_000),
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn complete_acceptance_requires_workflow_created_acceptance_file() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = in_progress_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        workflow_row(&conn, order_id, &gig_dir);

        let err = complete_acceptance(
            &conn,
            order_id,
            AcceptanceCompleteInput {
                completed_at: "2026-05-27T05:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("workflow-created acceptance"));
        let order = orders::find_by_id(&conn, order_id).unwrap();
        let workflow = order_workflow::find_by_order_id(&conn, order_id)
            .unwrap()
            .unwrap();
        assert_eq!(order.status, OrderStatus::InProgress);
        assert_eq!(workflow.acceptance_completed_at, None);
        assert!(!gig_dir.join("acceptance").exists());
    }

    #[test]
    fn check_acceptance_requires_required_headings() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = in_progress_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        let acceptance_dir = gig_dir.join("acceptance");
        std::fs::create_dir_all(&acceptance_dir).unwrap();
        std::fs::write(
            acceptance_dir.join("ACCEPTANCE.md"),
            "| item | method |
",
        )
        .unwrap();
        workflow_row(&conn, order_id, &gig_dir);

        let err = check_acceptance(&conn, order_id).unwrap_err();

        assert!(err.to_string().contains("acceptance headings"));
        let order = orders::find_by_id(&conn, order_id).unwrap();
        assert_eq!(order.status, OrderStatus::InProgress);
    }

    #[test]
    fn complete_acceptance_records_timestamp_and_sets_ready_to_deliver_without_creating_delivery() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = in_progress_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        let acceptance_dir = gig_dir.join("acceptance");
        std::fs::create_dir_all(&acceptance_dir).unwrap();
        std::fs::write(
            acceptance_dir.join("ACCEPTANCE.md"),
            "| 验收项 | 方法 | 证据 | 结论 |
| --- | --- | --- | --- |
| ok | inspect | file | pass |
",
        )
        .unwrap();
        workflow_row(&conn, order_id, &gig_dir);

        let completed = complete_acceptance(
            &conn,
            order_id,
            AcceptanceCompleteInput {
                completed_at: "2026-05-27T05:00:00Z",
            },
        )
        .unwrap();

        assert_eq!(completed.order.status, OrderStatus::ReadyToDeliver);
        assert_eq!(
            completed.workflow.acceptance_completed_at.as_deref(),
            Some("2026-05-27T05:00:00Z")
        );
        assert!(acceptance_dir.join("ACCEPTANCE.md").exists());
        assert!(!gig_dir.join("delivery").exists());
    }
}
