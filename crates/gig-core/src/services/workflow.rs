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
    require_workflow_created_file(Path::new(plan_md_path), "plan markdown")?;

    let tx = conn.unchecked_transaction()?;
    orders::update_status(&tx, order_id, OrderStatus::PlanReady, None, None)?;
    let order = orders::find_by_id(&tx, order_id)?;
    let workflow = order_workflow::record_plan_ready(&tx, order_id, input.ready_at)?;
    tx.commit()?;
    Ok(WorkflowResult { order, workflow })
}

pub fn approve_plan(
    conn: &Connection,
    order_id: i64,
    input: PlanApprovalInput<'_>,
) -> Result<WorkflowResult> {
    require_order_status(conn, order_id, OrderStatus::PlanReady)?;
    let tx = conn.unchecked_transaction()?;
    orders::update_status(&tx, order_id, OrderStatus::PlanApproved, None, None)?;
    let order = orders::find_by_id(&tx, order_id)?;
    let workflow = order_workflow::record_plan_approval(&tx, order_id, input.approved_at)?;
    tx.commit()?;
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
    let tx = conn.unchecked_transaction()?;
    let workflow = order_workflow::record_plan_rejection(&tx, order_id, reason, input.rejected_at)?;
    orders::update_status(&tx, order_id, OrderStatus::Accepted, None, None)?;
    let order = orders::find_by_id(&tx, order_id)?;
    tx.commit()?;
    Ok(WorkflowResult { order, workflow })
}

pub fn start_work(
    conn: &Connection,
    order_id: i64,
    input: WorkflowStartInput<'_>,
) -> Result<WorkflowResult> {
    require_order_status(conn, order_id, OrderStatus::PlanApproved)?;
    let tx = conn.unchecked_transaction()?;
    orders::update_status(&tx, order_id, OrderStatus::InProgress, None, None)?;
    let order = orders::find_by_id(&tx, order_id)?;
    let workflow = order_workflow::record_work_started(&tx, order_id, input.started_at)?;
    tx.commit()?;
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
    require_acceptance_items(&contents)?;

    Ok(WorkflowResult { order, workflow })
}

pub fn complete_acceptance(
    conn: &Connection,
    order_id: i64,
    input: AcceptanceCompleteInput<'_>,
) -> Result<WorkflowResult> {
    check_acceptance(conn, order_id)?;
    let tx = conn.unchecked_transaction()?;
    let workflow = order_workflow::record_acceptance_completed(&tx, order_id, input.completed_at)?;
    orders::update_status(&tx, order_id, OrderStatus::ReadyToDeliver, None, None)?;
    let order = orders::find_by_id(&tx, order_id)?;
    tx.commit()?;
    Ok(WorkflowResult { order, workflow })
}

fn require_acceptance_items(contents: &str) -> Result<()> {
    if contents.lines().any(|line| {
        let line = line.to_ascii_lowercase();
        line.contains("pending")
            || line.contains("待确认")
            || line.contains("待签字")
            || line.contains("待验收")
    }) {
        return Err(Error::Invalid(
            "acceptance contains a pending signoff or review".to_string(),
        ));
    }

    let mut rows_after_header = contents.lines().skip_while(|line| {
        let cells = markdown_table_cells(line);
        acceptance_column_indices(&cells).is_none()
    });

    let header = rows_after_header.next().ok_or_else(|| {
        Error::Invalid(
            "acceptance headings missing: require 验收项/方法/证据/结论 or English item/method/evidence/conclusion"
                .to_string(),
        )
    })?;
    let header_cells = markdown_table_cells(header);
    let columns = acceptance_column_indices(&header_cells).ok_or_else(|| {
        Error::Invalid(
            "acceptance headings missing: require 验收项/方法/证据/结论 or English item/method/evidence/conclusion"
                .to_string(),
        )
    })?;

    let mut item_count = 0;
    for line in rows_after_header {
        let cells = markdown_table_cells(line);
        if cells.is_empty() || is_markdown_separator_row(&cells) {
            continue;
        }

        item_count += 1;
        for (label, index) in [
            ("item", columns[0]),
            ("method", columns[1]),
            ("evidence", columns[2]),
            ("conclusion", columns[3]),
        ] {
            if cells.get(index).is_none_or(|cell| cell.trim().is_empty()) {
                return Err(Error::Invalid(format!(
                    "acceptance item incomplete: {label} is required"
                )));
            }
        }

        let conclusion = cells
            .get(columns[3])
            .map(String::as_str)
            .unwrap_or_default();
        if !acceptance_conclusion_is_pass_like(conclusion) {
            return Err(Error::Invalid(
                "acceptance item incomplete: conclusion must be pass-like and not blocked"
                    .to_string(),
            ));
        }
    }

    if item_count == 0 {
        return Err(Error::Invalid(
            "acceptance item incomplete: at least one acceptance item is required".to_string(),
        ));
    }

    Ok(())
}

fn markdown_table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return Vec::new();
    }

    trimmed
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn acceptance_column_indices(cells: &[String]) -> Option<[usize; 4]> {
    let normalized = cells
        .iter()
        .map(|cell| cell.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let item = find_heading(cells, &normalized, "验收项", "item")?;
    let method = find_heading(cells, &normalized, "方法", "method")?;
    let evidence = find_heading(cells, &normalized, "证据", "evidence")?;
    let conclusion = find_heading(cells, &normalized, "结论", "conclusion")?;
    Some([item, method, evidence, conclusion])
}

fn find_heading(
    cells: &[String],
    normalized: &[String],
    chinese: &str,
    english: &str,
) -> Option<usize> {
    cells
        .iter()
        .zip(normalized.iter())
        .position(|(cell, lower)| cell.trim() == chinese || lower == english)
}

fn is_markdown_separator_row(cells: &[String]) -> bool {
    cells.iter().all(|cell| {
        let trimmed = cell.trim();
        !trimmed.is_empty() && trimmed.chars().all(|ch| matches!(ch, '-' | ':' | ' '))
    })
}

fn acceptance_conclusion_is_pass_like(conclusion: &str) -> bool {
    let lower = conclusion.trim().to_ascii_lowercase();
    let blocked_terms = [
        "blocked",
        "block",
        "fail",
        "failed",
        "failing",
        "not pass",
        "not passed",
        "not ok",
        "not ready",
        "not done",
        "not complete",
        "not completed",
        "not success",
        "not successful",
        "not accepted",
        "no pass",
        "pending",
        "todo",
        "reject",
        "rejected",
        "incomplete",
        "未通过",
        "不通过",
        "失败",
        "阻塞",
        "待定",
        "未完成",
        "不合格",
        "未成功",
        "未验收",
    ];
    if blocked_terms
        .iter()
        .any(|term| lower.contains(term) || conclusion.contains(term))
    {
        return false;
    }

    let pass_terms = [
        "pass",
        "passed",
        "ok",
        "done",
        "complete",
        "completed",
        "success",
        "successful",
        "accepted",
        "ready",
        "通过",
        "完成",
        "合格",
        "成功",
        "已验收",
    ];
    pass_terms
        .iter()
        .any(|term| lower.contains(term) || conclusion.contains(term))
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
                index_path: None,
                job_path: Some(gig_dir.join("JOB.md").to_str().unwrap()),
                quote_path: Some(gig_dir.join("QUOTE.md").to_str().unwrap()),
                plan_md_path: Some(gig_dir.join("PLAN.md").to_str().unwrap()),
                plan_html_path: None,
                acceptance_path: Some(gig_dir.join("ACCEPTANCE.md").to_str().unwrap()),
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
        std::fs::create_dir_all(&gig_dir).unwrap();
        std::fs::write(gig_dir.join("PLAN.md"), "workflow-created plan").unwrap();
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
        assert!(gig_dir.join("PLAN.md").exists());
        assert!(!gig_dir.join("ACCEPTANCE.md").exists());
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
        std::fs::create_dir_all(&gig_dir).unwrap();
        std::fs::write(gig_dir.join("PLAN.md"), "workflow-created plan").unwrap();
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
        assert!(gig_dir.join("PLAN.md").exists());
        assert!(!gig_dir.join("ACCEPTANCE.md").exists());
    }

    #[test]
    fn reject_plan_records_reason_and_returns_order_to_accepted_without_rewriting_files() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let (order_id, gig_dir) = ready_order(&root, &conn);
        let plan_path = gig_dir.join("PLAN.md");
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
        assert_eq!(
            started.workflow.work_started_at.as_deref(),
            Some("2026-05-27T04:45:00Z")
        );
        assert_eq!(started.workflow.updated_at, "2026-05-27T04:45:00Z");
        assert!(gig_dir.join("PLAN.md").exists());
        assert!(!gig_dir.join("ACCEPTANCE.md").exists());
    }

    #[test]
    fn start_work_rolls_back_status_when_workflow_metadata_is_missing() {
        let conn = open_in_memory().unwrap();
        let order_id = accepted_order(&conn);
        orders::update_status(&conn, order_id, OrderStatus::PlanApproved, None, None).unwrap();

        let err = start_work(
            &conn,
            order_id,
            WorkflowStartInput {
                started_at: "2026-05-27T04:45:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("order_workflow"));
        let order = orders::find_by_id(&conn, order_id).unwrap();
        assert_eq!(order.status, OrderStatus::PlanApproved);
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
        assert!(!gig_dir.join("ACCEPTANCE.md").exists());
    }

    #[test]
    fn check_acceptance_requires_required_headings() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = in_progress_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        std::fs::create_dir_all(&gig_dir).unwrap();
        std::fs::write(
            gig_dir.join("ACCEPTANCE.md"),
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
    fn check_acceptance_rejects_incomplete_item_rows() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = in_progress_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        std::fs::create_dir_all(&gig_dir).unwrap();
        std::fs::write(
            gig_dir.join("ACCEPTANCE.md"),
            "| 验收项 | 方法 | 证据 | 结论 |
| --- | --- | --- | --- |
| crawler runs | run command |  | pass |
",
        )
        .unwrap();
        workflow_row(&conn, order_id, &gig_dir);

        let err = check_acceptance(&conn, order_id).unwrap_err();

        assert!(err.to_string().contains("acceptance item incomplete"));
        let order = orders::find_by_id(&conn, order_id).unwrap();
        assert_eq!(order.status, OrderStatus::InProgress);
    }

    #[test]
    fn check_acceptance_rejects_pending_signoff_outside_the_table() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = in_progress_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        std::fs::create_dir_all(&gig_dir).unwrap();
        std::fs::write(
            gig_dir.join("ACCEPTANCE.md"),
            "| Item | Method | Evidence | Conclusion |
| --- | --- | --- | --- |
| Export | test | output.csv | pass |

Human signoff pending.
",
        )
        .unwrap();
        workflow_row(&conn, order_id, &gig_dir);

        let err = check_acceptance(&conn, order_id).unwrap_err();

        assert!(err.to_string().contains("pending signoff"));
        let order = orders::find_by_id(&conn, order_id).unwrap();
        assert_eq!(order.status, OrderStatus::InProgress);
    }

    #[test]
    fn check_acceptance_rejects_blocked_conclusions() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = in_progress_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        std::fs::create_dir_all(&gig_dir).unwrap();
        std::fs::write(
            gig_dir.join("ACCEPTANCE.md"),
            "| 验收项 | 方法 | 证据 | 结论 |
| --- | --- | --- | --- |
| crawler runs | run command | log | blocked |
",
        )
        .unwrap();
        workflow_row(&conn, order_id, &gig_dir);

        let err = check_acceptance(&conn, order_id).unwrap_err();

        assert!(err.to_string().contains("conclusion must be pass-like"));
        let order = orders::find_by_id(&conn, order_id).unwrap();
        assert_eq!(order.status, OrderStatus::InProgress);
    }

    #[test]
    fn check_acceptance_rejects_negated_pass_like_conclusions() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = in_progress_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        std::fs::create_dir_all(&gig_dir).unwrap();
        std::fs::write(
            gig_dir.join("ACCEPTANCE.md"),
            "| 验收项 | 方法 | 证据 | 结论 |
| --- | --- | --- | --- |
| crawler runs | run command | log | not ready |
",
        )
        .unwrap();
        workflow_row(&conn, order_id, &gig_dir);

        let err = check_acceptance(&conn, order_id).unwrap_err();

        assert!(err.to_string().contains("conclusion must be pass-like"));
        let order = orders::find_by_id(&conn, order_id).unwrap();
        assert_eq!(order.status, OrderStatus::InProgress);
    }

    #[test]
    fn complete_acceptance_records_timestamp_and_sets_ready_to_deliver_without_creating_delivery() {
        let root = tempfile::tempdir().unwrap();
        let conn = open_in_memory().unwrap();
        let order_id = in_progress_order(&conn);
        let gig_dir = root.path().join("project/.gig");
        std::fs::create_dir_all(&gig_dir).unwrap();
        std::fs::write(
            gig_dir.join("ACCEPTANCE.md"),
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
        assert!(gig_dir.join("ACCEPTANCE.md").exists());
        assert!(!gig_dir.join("delivery").exists());
    }
}
