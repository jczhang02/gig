use crate::models::{OrderWorkflow, ProjectType};
use crate::{Error, Result};
use rusqlite::types::Type;
use rusqlite::{Connection, Row};
use std::str::FromStr;

const ALL_COLS: &str = "order_id, project_type, gig_dir, index_path, job_path, quote_path, \
     plan_md_path, plan_html_path, plan_ready_at, plan_approved_at, plan_rejected_at, \
     work_started_at, plan_rejection_reason, acceptance_path, acceptance_completed_at, \
     latest_delivery_dir, \
     latest_client_package_path, created_at, updated_at";

pub struct NewOrderWorkflow<'a> {
    pub order_id: i64,
    pub project_type: Option<ProjectType>,
    pub gig_dir: Option<&'a str>,
    pub index_path: Option<&'a str>,
    pub job_path: Option<&'a str>,
    pub quote_path: Option<&'a str>,
    pub plan_md_path: Option<&'a str>,
    pub plan_html_path: Option<&'a str>,
    pub acceptance_path: Option<&'a str>,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

fn conversion_error(err: crate::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(err))
}

fn map_project_type(value: Option<String>) -> rusqlite::Result<Option<ProjectType>> {
    if let Some(value) = value {
        ProjectType::from_str(&value)
            .map(Some)
            .map_err(conversion_error)
    } else {
        Ok(None)
    }
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<OrderWorkflow> {
    Ok(OrderWorkflow {
        order_id: row.get("order_id")?,
        project_type: map_project_type(row.get("project_type")?)?,
        gig_dir: row.get("gig_dir")?,
        index_path: row.get("index_path")?,
        job_path: row.get("job_path")?,
        quote_path: row.get("quote_path")?,
        plan_md_path: row.get("plan_md_path")?,
        plan_html_path: row.get("plan_html_path")?,
        plan_ready_at: row.get("plan_ready_at")?,
        plan_approved_at: row.get("plan_approved_at")?,
        plan_rejected_at: row.get("plan_rejected_at")?,
        work_started_at: row.get("work_started_at")?,
        plan_rejection_reason: row.get("plan_rejection_reason")?,
        acceptance_path: row.get("acceptance_path")?,
        acceptance_completed_at: row.get("acceptance_completed_at")?,
        latest_delivery_dir: row.get("latest_delivery_dir")?,
        latest_client_package_path: row.get("latest_client_package_path")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn insert(conn: &Connection, new: &NewOrderWorkflow<'_>) -> Result<OrderWorkflow> {
    conn.execute(
        "INSERT INTO order_workflow \
         (order_id, project_type, gig_dir, index_path, job_path, quote_path, plan_md_path, \
          plan_html_path, acceptance_path, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        (
            new.order_id,
            new.project_type.map(|project_type| project_type.as_str()),
            new.gig_dir,
            new.index_path,
            new.job_path,
            new.quote_path,
            new.plan_md_path,
            new.plan_html_path,
            new.acceptance_path,
            new.created_at,
            new.updated_at,
        ),
    )?;
    Ok(find_by_order_id(conn, new.order_id)?.expect("inserted order_workflow row missing"))
}

pub fn find_by_order_id(conn: &Connection, order_id: i64) -> Result<Option<OrderWorkflow>> {
    let sql = format!("SELECT {ALL_COLS} FROM order_workflow WHERE order_id = ?1 LIMIT 1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map((order_id,), map_row)?;
    if let Some(row) = rows.next() {
        Ok(Some(row?))
    } else {
        Ok(None)
    }
}

pub fn list(conn: &Connection) -> Result<Vec<OrderWorkflow>> {
    let sql = format!("SELECT {ALL_COLS} FROM order_workflow ORDER BY order_id ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], map_row)?;
    rows.map(|row| row.map_err(Into::into)).collect()
}

fn fetch_updated(conn: &Connection, order_id: i64, changed: usize) -> Result<OrderWorkflow> {
    if changed == 0 {
        return Err(Error::Invalid(format!(
            "missing order_workflow for order {order_id}"
        )));
    }

    find_by_order_id(conn, order_id)?.ok_or_else(|| {
        Error::Invalid(format!(
            "missing order_workflow for order {order_id} after update"
        ))
    })
}

pub fn record_plan_ready(
    conn: &Connection,
    order_id: i64,
    updated_at: &str,
) -> Result<OrderWorkflow> {
    let changed = conn.execute(
        "UPDATE order_workflow \
         SET plan_ready_at = ?2, updated_at = ?2 \
         WHERE order_id = ?1",
        (order_id, updated_at),
    )?;
    fetch_updated(conn, order_id, changed)
}

pub fn record_plan_approval(
    conn: &Connection,
    order_id: i64,
    updated_at: &str,
) -> Result<OrderWorkflow> {
    let changed = conn.execute(
        "UPDATE order_workflow \
         SET plan_approved_at = ?2, updated_at = ?2 \
         WHERE order_id = ?1",
        (order_id, updated_at),
    )?;
    fetch_updated(conn, order_id, changed)
}

pub fn record_plan_rejection(
    conn: &Connection,
    order_id: i64,
    reason: &str,
    updated_at: &str,
) -> Result<OrderWorkflow> {
    let changed = conn.execute(
        "UPDATE order_workflow \
         SET plan_rejected_at = ?2, plan_rejection_reason = ?3, updated_at = ?2 \
         WHERE order_id = ?1",
        (order_id, updated_at, reason),
    )?;
    fetch_updated(conn, order_id, changed)
}

pub fn record_work_started(
    conn: &Connection,
    order_id: i64,
    updated_at: &str,
) -> Result<OrderWorkflow> {
    let changed = conn.execute(
        "UPDATE order_workflow \
         SET work_started_at = ?2, updated_at = ?2 \
         WHERE order_id = ?1",
        (order_id, updated_at),
    )?;
    fetch_updated(conn, order_id, changed)
}

pub fn record_acceptance_completed(
    conn: &Connection,
    order_id: i64,
    updated_at: &str,
) -> Result<OrderWorkflow> {
    let changed = conn.execute(
        "UPDATE order_workflow \
         SET acceptance_completed_at = ?2, updated_at = ?2 \
         WHERE order_id = ?1",
        (order_id, updated_at),
    )?;
    fetch_updated(conn, order_id, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::{OrderStatus, ProjectType};
    use crate::repo::orders::{self, NewOrder};

    fn sample_order(conn: &rusqlite::Connection) -> i64 {
        orders::insert(
            conn,
            &NewOrder {
                slug: Some("order-1"),
                external_id: None,
                title: "Accepted order",
                client_id: None,
                source_org: None,
                source_id: None,
                project_type: None,
                status: OrderStatus::Accepted,
                quoted_price: Some(15_000),
                final_price: None,
                my_cut_ratio: 0.6,
                currency: "CNY",
                notes: None,
                created_at: 1,
                accepted_at: Some(1),
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn insert_and_find_order_workflow() {
        let conn = open_in_memory().unwrap();
        let order_id = sample_order(&conn);

        let workflow = insert(
            &conn,
            &NewOrderWorkflow {
                order_id,
                project_type: Some(ProjectType::FrontendWeb),
                gig_dir: Some("/work/project/.gig"),
                index_path: Some("/work/project/.gig/INDEX.html"),
                job_path: Some("/work/project/.gig/JOB.md"),
                quote_path: Some("/work/project/.gig/QUOTE.md"),
                plan_md_path: None,
                plan_html_path: None,
                acceptance_path: None,
                created_at: "2026-05-27T00:00:00Z",
                updated_at: "2026-05-27T00:00:00Z",
            },
        )
        .unwrap();

        assert_eq!(workflow.order_id, order_id);
        assert_eq!(workflow.project_type, Some(ProjectType::FrontendWeb));
        assert_eq!(
            workflow.index_path.as_deref(),
            Some("/work/project/.gig/INDEX.html")
        );

        let found = find_by_order_id(&conn, order_id).unwrap().unwrap();
        assert_eq!(found, workflow);
    }

    #[test]
    fn update_plan_rejection_records_reason() {
        let conn = open_in_memory().unwrap();
        let order_id = sample_order(&conn);
        insert(
            &conn,
            &NewOrderWorkflow {
                order_id,
                project_type: Some(ProjectType::FrontendWeb),
                gig_dir: Some("/work/project/.gig"),
                index_path: None,
                job_path: None,
                quote_path: None,
                plan_md_path: None,
                plan_html_path: None,
                acceptance_path: None,
                created_at: "2026-05-27T00:00:00Z",
                updated_at: "2026-05-27T00:00:00Z",
            },
        )
        .unwrap();

        let rejected =
            record_plan_rejection(&conn, order_id, "scope missing", "2026-05-27T03:00:00Z")
                .unwrap();

        assert_eq!(
            rejected.plan_rejected_at.as_deref(),
            Some("2026-05-27T03:00:00Z")
        );
        assert_eq!(
            rejected.plan_rejection_reason.as_deref(),
            Some("scope missing")
        );
    }

    #[test]
    fn workflow_update_helpers_return_error_when_row_missing() {
        let conn = open_in_memory().unwrap();
        let order_id = sample_order(&conn);

        let err = record_plan_ready(&conn, order_id, "2026-05-27T01:00:00Z").unwrap_err();
        assert!(err.to_string().contains("order_workflow"));

        let err = record_plan_approval(&conn, order_id, "2026-05-27T02:00:00Z").unwrap_err();
        assert!(err.to_string().contains("order_workflow"));

        let err = record_plan_rejection(&conn, order_id, "scope missing", "2026-05-27T03:00:00Z")
            .unwrap_err();
        assert!(err.to_string().contains("order_workflow"));

        let err = record_work_started(&conn, order_id, "2026-05-27T04:00:00Z").unwrap_err();
        assert!(err.to_string().contains("order_workflow"));

        let err = record_acceptance_completed(&conn, order_id, "2026-05-27T05:00:00Z").unwrap_err();
        assert!(err.to_string().contains("order_workflow"));
    }
}
