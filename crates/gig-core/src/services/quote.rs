use crate::config::Paths;
use crate::models::{Order, OrderStatus, OrderWorkflow, ProjectType, QuoteDraft, QuoteDraftStatus};
use crate::repo::order_workflow::{self, NewOrderWorkflow};
use crate::repo::orders::{self, NewOrder};
use crate::repo::quote_drafts::{self, NewQuoteDraft};
use crate::{Error, Result};
use rusqlite::Connection;
use std::path::Path;

pub struct QuoteDraftInput<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub client_label: Option<&'a str>,
    pub source_org: Option<&'a str>,
    pub project_type: ProjectType,
    pub summary: &'a str,
    pub created_at: &'a str,
}

pub struct QuotePriceInput<'a> {
    pub quote_min: i64,
    pub quote_recommended: i64,
    pub quote_max: i64,
    pub updated_at: &'a str,
}

pub struct QuoteAcceptInput<'a> {
    pub project_dir: &'a Path,
    pub accepted_at: &'a str,
    pub accepted_at_unix: i64,
    pub my_cut_ratio: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuoteAcceptResult {
    pub quote_draft: QuoteDraft,
    pub order: Order,
    pub workflow: OrderWorkflow,
}

pub fn record_quote_draft(
    conn: &Connection,
    paths: &Paths,
    input: &QuoteDraftInput<'_>,
) -> Result<QuoteDraft> {
    let quote_path = paths.data_dir.join("quotes").join(input.slug);
    let xdg_path = quote_path.to_string_lossy().into_owned();
    quote_drafts::insert(
        conn,
        &NewQuoteDraft {
            slug: input.slug,
            title: input.title,
            client_label: input.client_label,
            source_org: input.source_org,
            project_type: input.project_type,
            status: QuoteDraftStatus::QuoteDraft,
            summary: input.summary,
            quote_min: None,
            quote_recommended: None,
            quote_max: None,
            currency: "CNY",
            xdg_path: &xdg_path,
            created_at: input.created_at,
            updated_at: input.created_at,
        },
    )
}

pub fn price_quote_draft(
    conn: &Connection,
    id: i64,
    input: QuotePriceInput<'_>,
) -> Result<QuoteDraft> {
    let draft = quote_drafts::find_by_id(conn, id)?;
    if !matches!(
        draft.status,
        QuoteDraftStatus::QuoteDraft | QuoteDraftStatus::NeedsClarification
    ) {
        return Err(Error::Invalid(format!(
            "quote draft must be quote_draft or needs_clarification before pricing: {}",
            draft.status
        )));
    }

    quote_drafts::update_pricing(
        conn,
        id,
        input.quote_min,
        input.quote_recommended,
        input.quote_max,
        input.updated_at,
    )
}

pub fn mark_quote_sent(conn: &Connection, id: i64, updated_at: &str) -> Result<QuoteDraft> {
    let draft = quote_drafts::find_by_id(conn, id)?;
    if draft.status != QuoteDraftStatus::Quoted {
        return Err(Error::Invalid(format!(
            "quote draft must be quoted before marking sent: {}",
            draft.status
        )));
    }

    quote_drafts::mark_sent(conn, id, updated_at)
}

pub fn drop_quote_draft(
    conn: &Connection,
    id: i64,
    drop_reason: &str,
    updated_at: &str,
) -> Result<QuoteDraft> {
    if drop_reason.trim().is_empty() {
        return Err(Error::Invalid("drop_reason must not be empty".into()));
    }

    let draft = quote_drafts::find_by_id(conn, id)?;
    if matches!(
        draft.status,
        QuoteDraftStatus::Accepted | QuoteDraftStatus::Dropped
    ) {
        return Err(Error::Invalid(format!(
            "quote draft is terminal and cannot be dropped: {}",
            draft.status
        )));
    }

    quote_drafts::mark_dropped(conn, id, drop_reason, updated_at)
}

pub fn accept_quote_draft(
    conn: &Connection,
    id: i64,
    input: QuoteAcceptInput<'_>,
) -> Result<QuoteAcceptResult> {
    let gig_dir = input.project_dir.join(".gig");
    let job_path = gig_dir.join("JOB.md");
    let quote_path = gig_dir.join("QUOTE.md");
    require_workflow_created_file(&job_path, "JOB.md")?;
    require_workflow_created_file(&quote_path, "QUOTE.md")?;

    let draft = quote_drafts::find_by_id(conn, id)?;
    if draft.status != QuoteDraftStatus::Quoted {
        return Err(Error::Invalid(format!(
            "quote draft must be quoted before acceptance: {}",
            draft.status
        )));
    }

    let quoted_price = draft
        .quote_recommended
        .or(draft.quote_min)
        .or(draft.quote_max);
    let order = orders::insert(
        conn,
        &NewOrder {
            slug: Some(draft.slug.as_str()),
            external_id: None,
            title: draft.title.as_str(),
            client_id: None,
            source_org: draft.source_org.as_deref(),
            source_id: None,
            project_type: Some(draft.project_type),
            status: OrderStatus::Accepted,
            quoted_price,
            final_price: None,
            my_cut_ratio: input.my_cut_ratio,
            currency: draft.currency.as_str(),
            notes: Some(draft.summary.as_str()),
            created_at: input.accepted_at_unix,
            accepted_at: Some(input.accepted_at_unix),
        },
    )?;

    let workflow_paths = WorkflowPaths::new(&gig_dir);
    let workflow = order_workflow::insert(
        conn,
        &NewOrderWorkflow {
            order_id: order.id,
            project_type: Some(draft.project_type),
            gig_dir: Some(workflow_paths.gig_dir.as_str()),
            index_path: Some(workflow_paths.index_path.as_str()),
            job_path: Some(workflow_paths.job_path.as_str()),
            quote_path: Some(workflow_paths.quote_path.as_str()),
            plan_md_path: Some(workflow_paths.plan_md_path.as_str()),
            plan_html_path: Some(workflow_paths.plan_html_path.as_str()),
            acceptance_path: Some(workflow_paths.acceptance_path.as_str()),
            created_at: input.accepted_at,
            updated_at: input.accepted_at,
        },
    )?;
    let quote_draft = quote_drafts::mark_accepted(conn, id, order.id, input.accepted_at)?;

    Ok(QuoteAcceptResult {
        quote_draft,
        order,
        workflow,
    })
}

fn require_workflow_created_file(path: &Path, label: &str) -> Result<()> {
    if path.is_file() {
        return Ok(());
    }
    Err(Error::Invalid(format!(
        "missing workflow-created {label}: {}",
        path.display()
    )))
}

struct WorkflowPaths {
    gig_dir: String,
    index_path: String,
    job_path: String,
    quote_path: String,
    plan_md_path: String,
    plan_html_path: String,
    acceptance_path: String,
}

impl WorkflowPaths {
    fn new(gig_dir: &Path) -> Self {
        Self {
            gig_dir: path_string(gig_dir),
            index_path: path_string(&gig_dir.join("INDEX.html")),
            job_path: path_string(&gig_dir.join("JOB.md")),
            quote_path: path_string(&gig_dir.join("QUOTE.md")),
            plan_md_path: path_string(&gig_dir.join("plan/PLAN.md")),
            plan_html_path: path_string(&gig_dir.join("plan/PLAN.html")),
            acceptance_path: path_string(&gig_dir.join("acceptance/ACCEPTANCE.md")),
        }
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;
    use crate::db::open_in_memory;
    use crate::models::{OrderStatus, ProjectType, QuoteDraftStatus};
    use crate::repo::{order_workflow, quote_drafts};

    fn input<'a>() -> QuoteDraftInput<'a> {
        QuoteDraftInput {
            slug: "draft-1",
            title: "Build a crawler",
            client_label: Some("Client A"),
            source_org: Some("wechat"),
            project_type: ProjectType::Crawler,
            summary: "Need a crawler for listings",
            created_at: "2026-05-27T00:00:00Z",
        }
    }

    #[test]
    fn record_quote_draft_stores_xdg_path_without_creating_files() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(root.path());
        let conn = open_in_memory().unwrap();

        let draft = record_quote_draft(&conn, &paths, &input()).unwrap();
        let quote_dir = paths.data_dir.join("quotes").join("draft-1");

        assert_eq!(draft.slug, "draft-1");
        assert_eq!(draft.status, QuoteDraftStatus::QuoteDraft);
        assert_eq!(draft.xdg_path, quote_dir.to_string_lossy());
        assert!(!quote_dir.exists());
        assert!(!quote_dir.join("JOB.md").exists());

        let stored = quote_drafts::find_by_slug(&conn, "draft-1")
            .unwrap()
            .unwrap();
        assert_eq!(stored, draft);
    }

    #[test]
    fn price_quote_draft_marks_quoted_without_accepting() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(root.path());
        let conn = open_in_memory().unwrap();
        let draft = record_quote_draft(&conn, &paths, &input()).unwrap();

        let priced = price_quote_draft(
            &conn,
            draft.id,
            QuotePriceInput {
                quote_min: 10_000,
                quote_recommended: 15_000,
                quote_max: 20_000,
                updated_at: "2026-05-27T01:00:00Z",
            },
        )
        .unwrap();

        assert_eq!(priced.status, QuoteDraftStatus::Quoted);
        assert_eq!(priced.quote_min, Some(10_000));
        assert_eq!(priced.quote_recommended, Some(15_000));
        assert_eq!(priced.quote_max, Some(20_000));
        assert_eq!(priced.quoted_at.as_deref(), Some("2026-05-27T01:00:00Z"));
        assert_eq!(priced.accepted_at, None);
    }

    #[test]
    fn mark_quote_sent_records_timestamp_without_accepting() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(root.path());
        let conn = open_in_memory().unwrap();
        let draft = record_quote_draft(&conn, &paths, &input()).unwrap();
        let priced = price_quote_draft(
            &conn,
            draft.id,
            QuotePriceInput {
                quote_min: 10_000,
                quote_recommended: 15_000,
                quote_max: 20_000,
                updated_at: "2026-05-27T01:00:00Z",
            },
        )
        .unwrap();

        let sent = mark_quote_sent(&conn, priced.id, "2026-05-27T01:30:00Z").unwrap();

        assert_eq!(sent.status, QuoteDraftStatus::Quoted);
        assert_eq!(sent.sent_at.as_deref(), Some("2026-05-27T01:30:00Z"));
        assert_eq!(sent.accepted_at, None);
    }

    #[test]
    fn drop_quote_draft_requires_non_empty_reason() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(root.path());
        let conn = open_in_memory().unwrap();
        let draft = record_quote_draft(&conn, &paths, &input()).unwrap();

        let err = drop_quote_draft(&conn, draft.id, "  ", "2026-05-27T02:00:00Z").unwrap_err();
        assert!(err.to_string().contains("drop_reason"));

        let stored = quote_drafts::find_by_id(&conn, draft.id).unwrap();
        assert_eq!(stored.status, QuoteDraftStatus::QuoteDraft);
    }

    #[test]
    fn quote_state_gates_reject_invalid_transitions_without_side_effects() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(root.path());
        let conn = open_in_memory().unwrap();

        let draft = record_quote_draft(&conn, &paths, &input()).unwrap();
        let err = mark_quote_sent(&conn, draft.id, "2026-05-27T00:30:00Z").unwrap_err();
        assert!(err.to_string().contains("quoted"));
        let stored = quote_drafts::find_by_id(&conn, draft.id).unwrap();
        assert_eq!(stored.status, QuoteDraftStatus::QuoteDraft);
        assert_eq!(stored.sent_at, None);

        let priced = price_quote_draft(
            &conn,
            draft.id,
            QuotePriceInput {
                quote_min: 10_000,
                quote_recommended: 15_000,
                quote_max: 20_000,
                updated_at: "2026-05-27T01:00:00Z",
            },
        )
        .unwrap();
        let dropped =
            drop_quote_draft(&conn, priced.id, "client declined", "2026-05-27T01:30:00Z").unwrap();
        assert_eq!(dropped.status, QuoteDraftStatus::Dropped);

        let err = price_quote_draft(
            &conn,
            dropped.id,
            QuotePriceInput {
                quote_min: 11_000,
                quote_recommended: 16_000,
                quote_max: 21_000,
                updated_at: "2026-05-27T02:00:00Z",
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("quote_draft"));
        let err = drop_quote_draft(&conn, dropped.id, "again", "2026-05-27T02:30:00Z").unwrap_err();
        assert!(err.to_string().contains("terminal"));
        let stored = quote_drafts::find_by_id(&conn, dropped.id).unwrap();
        assert_eq!(stored.status, QuoteDraftStatus::Dropped);
        assert_eq!(stored.quote_recommended, Some(15_000));
        assert_eq!(stored.drop_reason.as_deref(), Some("client declined"));

        let accepted_draft = record_quote_draft(
            &conn,
            &paths,
            &QuoteDraftInput {
                slug: "accepted-draft",
                title: "Accepted draft",
                ..input()
            },
        )
        .unwrap();
        let priced = price_quote_draft(
            &conn,
            accepted_draft.id,
            QuotePriceInput {
                quote_min: 20_000,
                quote_recommended: 25_000,
                quote_max: 30_000,
                updated_at: "2026-05-27T03:00:00Z",
            },
        )
        .unwrap();
        let project_dir = root.path().join("accepted-gated-project");
        let gig_dir = project_dir.join(".gig");
        std::fs::create_dir_all(&gig_dir).unwrap();
        std::fs::write(gig_dir.join("JOB.md"), "workflow-created job").unwrap();
        std::fs::write(gig_dir.join("QUOTE.md"), "workflow-created quote").unwrap();
        let accepted = accept_quote_draft(
            &conn,
            priced.id,
            QuoteAcceptInput {
                project_dir: &project_dir,
                accepted_at: "2026-05-27T04:00:00Z",
                accepted_at_unix: 1_700_000_000,
                my_cut_ratio: 0.6,
            },
        )
        .unwrap();

        let err = price_quote_draft(
            &conn,
            accepted.quote_draft.id,
            QuotePriceInput {
                quote_min: 21_000,
                quote_recommended: 26_000,
                quote_max: 31_000,
                updated_at: "2026-05-27T05:00:00Z",
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("accepted"));
        let err = drop_quote_draft(
            &conn,
            accepted.quote_draft.id,
            "too late",
            "2026-05-27T05:30:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("terminal"));
        let stored = quote_drafts::find_by_id(&conn, accepted.quote_draft.id).unwrap();
        assert_eq!(stored.status, QuoteDraftStatus::Accepted);
        assert_eq!(stored.quote_recommended, Some(25_000));
        assert_eq!(stored.promoted_order_id, Some(accepted.order.id));
    }

    #[test]
    fn accept_quote_draft_promotes_order_and_records_expected_workflow_paths() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(root.path());
        let conn = open_in_memory().unwrap();
        let draft = record_quote_draft(&conn, &paths, &input()).unwrap();
        let priced = price_quote_draft(
            &conn,
            draft.id,
            QuotePriceInput {
                quote_min: 10_000,
                quote_recommended: 15_000,
                quote_max: 20_000,
                updated_at: "2026-05-27T01:00:00Z",
            },
        )
        .unwrap();
        let project_dir = root.path().join("accepted-project");
        let gig_dir = project_dir.join(".gig");
        std::fs::create_dir_all(&gig_dir).unwrap();
        std::fs::write(gig_dir.join("JOB.md"), "workflow-created job").unwrap();
        std::fs::write(gig_dir.join("QUOTE.md"), "workflow-created quote").unwrap();

        let accepted = accept_quote_draft(
            &conn,
            priced.id,
            QuoteAcceptInput {
                project_dir: &project_dir,
                accepted_at: "2026-05-27T02:00:00Z",
                accepted_at_unix: 1_700_000_000,
                my_cut_ratio: 0.6,
            },
        )
        .unwrap();

        assert_eq!(accepted.quote_draft.status, QuoteDraftStatus::Accepted);
        assert_eq!(
            accepted.quote_draft.promoted_order_id,
            Some(accepted.order.id)
        );
        assert_eq!(
            accepted.quote_draft.accepted_at.as_deref(),
            Some("2026-05-27T02:00:00Z")
        );
        assert_eq!(accepted.order.status, OrderStatus::Accepted);
        assert_eq!(accepted.order.project_type, Some(ProjectType::Crawler));
        assert_eq!(accepted.order.title, "Build a crawler");
        assert_eq!(accepted.order.quoted_price, Some(15_000));
        assert_eq!(accepted.order.accepted_at, Some(1_700_000_000));
        assert_eq!(accepted.workflow.order_id, accepted.order.id);
        assert_eq!(
            accepted.workflow.gig_dir.as_deref(),
            Some(gig_dir.to_str().unwrap())
        );
        assert_eq!(
            accepted.workflow.job_path.as_deref(),
            Some(gig_dir.join("JOB.md").to_str().unwrap())
        );
        assert_eq!(
            accepted.workflow.quote_path.as_deref(),
            Some(gig_dir.join("QUOTE.md").to_str().unwrap())
        );
        assert_eq!(
            accepted.workflow.plan_md_path.as_deref(),
            Some(gig_dir.join("plan/PLAN.md").to_str().unwrap())
        );
        assert_eq!(
            accepted.workflow.plan_html_path.as_deref(),
            Some(gig_dir.join("plan/PLAN.html").to_str().unwrap())
        );
        assert_eq!(
            accepted.workflow.acceptance_path.as_deref(),
            Some(gig_dir.join("acceptance/ACCEPTANCE.md").to_str().unwrap())
        );
        assert!(!gig_dir.join("plan/PLAN.md").exists());
        assert!(!gig_dir.join("plan/PLAN.html").exists());
        assert!(!gig_dir.join("acceptance/ACCEPTANCE.md").exists());

        let stored_workflow = order_workflow::find_by_order_id(&conn, accepted.order.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_workflow, accepted.workflow);
    }

    #[test]
    fn accept_quote_draft_requires_workflow_created_quote_files_before_promotion() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(root.path());
        let conn = open_in_memory().unwrap();
        let draft = record_quote_draft(&conn, &paths, &input()).unwrap();
        let project_dir = root.path().join("missing-workflow-files");

        let err = accept_quote_draft(
            &conn,
            draft.id,
            QuoteAcceptInput {
                project_dir: &project_dir,
                accepted_at: "2026-05-27T02:00:00Z",
                accepted_at_unix: 1_700_000_000,
                my_cut_ratio: 0.6,
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("workflow-created"));
        let stored = quote_drafts::find_by_id(&conn, draft.id).unwrap();
        assert_eq!(stored.status, QuoteDraftStatus::QuoteDraft);
        assert_eq!(stored.promoted_order_id, None);
        assert!(!project_dir.join(".gig").exists());
    }
}
