//! Dashboard service: build the "today's focus" view for `gig ls`.

use crate::models::{
    DeliveryPackageStatus, Order, OrderStatus, OrderWorkflow, QuoteDraft, QuoteDraftStatus,
};
use crate::repo::orders::{list, ListFilter};
use crate::repo::{delivery_packages, order_workflow, quote_drafts};
use crate::{Error, Result};
use rusqlite::Connection;

// Thresholds (days). Hardcoded for v0.1; configurable later.
const DELIVERED_UNPAID_DAYS: i64 = 3;
const IN_PROGRESS_STALE_DAYS: i64 = 5;
const LEAD_STALE_DAYS: i64 = 2;

/// A single item shown in the dashboard focus view.
#[derive(Debug, Clone)]
pub struct DashboardItem {
    pub order: Order,
    pub workflow: Option<OrderWorkflow>,
    pub next_action: &'static str,
    /// A human-readable alert string, e.g. "已交付 3 天未收款".
    pub alert: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DashboardQuoteItem {
    pub draft: QuoteDraft,
    pub next_action: &'static str,
}

/// Summary numbers shown at the bottom of the dashboard.
#[derive(Debug, Clone, Default)]
pub struct DashboardSummary {
    /// Sum of my_cut_amount for paid orders this calendar month (minor units).
    pub this_month_income: i64,
    /// Total non-terminal order count (lead + negotiating + accepted + in_progress + delivered).
    pub orders_count: usize,
    /// Delivered-but-unpaid order count.
    pub pending_count: usize,
}

/// The full dashboard result.
#[derive(Debug, Clone)]
pub struct Dashboard {
    pub quote_items: Vec<DashboardQuoteItem>,
    pub focus_items: Vec<DashboardItem>,
    pub summary: DashboardSummary,
}

/// Build the dashboard for the given `now` unix timestamp.
pub fn build_dashboard(conn: &Connection, now: i64) -> Result<Dashboard> {
    let all = list(conn, &ListFilter { status: None })?;
    let quote_items = quote_drafts::list(conn)?
        .into_iter()
        .filter_map(|draft| {
            quote_next_action(&draft).map(|next_action| DashboardQuoteItem { draft, next_action })
        })
        .collect();

    // Compute month boundaries for income summary.
    let (month_start, month_end) = current_month_bounds(now);

    let mut focus_items = Vec::new();
    let mut summary = DashboardSummary::default();

    for order in &all {
        let is_unpaid = order.paid_at.is_none();
        let is_archived_unpaid = order.status == OrderStatus::Archived && is_unpaid;

        // Count active orders (not terminal, plus archived-unpaid).
        match order.status {
            OrderStatus::Lead
            | OrderStatus::Negotiating
            | OrderStatus::Accepted
            | OrderStatus::PlanReady
            | OrderStatus::PlanApproved
            | OrderStatus::InProgress
            | OrderStatus::ReadyToDeliver
            | OrderStatus::Delivered
            | OrderStatus::Revision => {
                summary.orders_count += 1;
            }
            OrderStatus::Archived if is_unpaid => {
                summary.orders_count += 1;
            }
            _ => {}
        }

        // Count pending (delivered/revision/archived but unpaid).
        if is_unpaid
            && matches!(
                order.status,
                OrderStatus::Delivered | OrderStatus::Revision | OrderStatus::Archived
            )
        {
            summary.pending_count += 1;
        }

        // This-month income: any order with paid_at in this month.
        if let Some(paid_at) = order.paid_at {
            if paid_at >= month_start && paid_at < month_end {
                if let Some(cut) = order.my_cut_amount() {
                    summary.this_month_income += cut;
                }
            }
        }

        // Alert logic.
        let alert = compute_alert(order, conn, now)?;
        let workflow = order_workflow::find_by_order_id(conn, order.id)?;
        let next_action = order_next_action(conn, order, workflow.as_ref())?;

        // Include in focus list if it has an alert or is in an active state.
        let show = alert.is_some()
            || matches!(
                order.status,
                OrderStatus::Lead
                    | OrderStatus::Negotiating
                    | OrderStatus::Accepted
                    | OrderStatus::PlanReady
                    | OrderStatus::PlanApproved
                    | OrderStatus::InProgress
                    | OrderStatus::ReadyToDeliver
                    | OrderStatus::Delivered
                    | OrderStatus::Revision
            )
            || is_archived_unpaid;

        if show {
            focus_items.push(DashboardItem {
                order: order.clone(),
                workflow,
                next_action,
                alert,
            });
        }
    }

    // Sort: items with alerts first, then by id desc.
    focus_items.sort_by(|a, b| {
        let a_alert = a.alert.is_some();
        let b_alert = b.alert.is_some();
        b_alert.cmp(&a_alert).then(b.order.id.cmp(&a.order.id))
    });

    Ok(Dashboard {
        quote_items,
        focus_items,
        summary,
    })
}

pub fn quote_next_action(draft: &QuoteDraft) -> Option<&'static str> {
    match draft.status {
        QuoteDraftStatus::QuoteDraft => Some("price_quote"),
        QuoteDraftStatus::NeedsClarification => Some("collect_missing_facts"),
        QuoteDraftStatus::Quoted if draft.sent_at.is_some() => Some("wait_client_decision"),
        QuoteDraftStatus::Quoted => Some("send_quote"),
        QuoteDraftStatus::Accepted | QuoteDraftStatus::Dropped => None,
    }
}

pub fn order_next_action(
    conn: &Connection,
    order: &Order,
    workflow: Option<&OrderWorkflow>,
) -> Result<&'static str> {
    let workflow_required = matches!(
        order.status,
        OrderStatus::Accepted
            | OrderStatus::PlanReady
            | OrderStatus::PlanApproved
            | OrderStatus::InProgress
            | OrderStatus::ReadyToDeliver
    );
    if workflow_required && workflow.is_none() {
        return Ok("legacy_workflow_metadata_missing");
    }

    let next_action = match order.status {
        OrderStatus::Lead => "qualify_lead",
        OrderStatus::Negotiating => "resolve_quote",
        OrderStatus::Accepted => "prepare_plan",
        OrderStatus::PlanReady => "approve_plan",
        OrderStatus::PlanApproved => "start_work",
        OrderStatus::InProgress => "complete_acceptance",
        OrderStatus::ReadyToDeliver => ready_to_deliver_next_action(conn, order.id)?,
        OrderStatus::Delivered => "collect_payment",
        OrderStatus::Revision => "complete_revision",
        OrderStatus::Paid => "archive_order",
        OrderStatus::Archived | OrderStatus::Cancelled => "none",
    };
    Ok(next_action)
}

fn ready_to_deliver_next_action(conn: &Connection, order_id: i64) -> Result<&'static str> {
    let has_validated_package = delivery_packages::list_for_order(conn, order_id)?
        .into_iter()
        .any(|package| package.status == DeliveryPackageStatus::Validated);
    Ok(if has_validated_package {
        "send_package"
    } else {
        "check_package"
    })
}

fn compute_alert(order: &Order, conn: &Connection, now: i64) -> Result<Option<String>> {
    let days_since = |ts: i64| -> i64 { (now - ts) / 86400 };

    match order.status {
        OrderStatus::Delivered if order.paid_at.is_none() => {
            // delivered with no paid_at for 3+ days.
            let since = order
                .delivered_at
                .map(days_since)
                .unwrap_or(days_since(order.created_at));
            if since >= DELIVERED_UNPAID_DAYS {
                return Ok(Some(format!("delivered {} days ago, unpaid", since)));
            }
        }
        OrderStatus::InProgress => {
            // in_progress with no changes or price_history in 5+ days.
            let last_activity = last_activity_ts(conn, order.id)?;
            let base_ts = last_activity.unwrap_or(order.created_at);
            let since = days_since(base_ts);
            if since >= IN_PROGRESS_STALE_DAYS {
                return Ok(Some(format!("no updates for {} days", since)));
            }
        }
        OrderStatus::Lead => {
            // lead sitting for 2+ days.
            let since = days_since(order.created_at);
            if since >= LEAD_STALE_DAYS {
                return Ok(Some(format!("lead sitting for {} days", since)));
            }
        }
        OrderStatus::Archived if order.paid_at.is_none() => {
            // archived but never paid.
            let since = order
                .archived_at
                .map(days_since)
                .unwrap_or(days_since(order.created_at));
            return Ok(Some(format!("archived {} days ago, unpaid", since)));
        }
        _ => {}
    }

    Ok(None)
}

/// Returns the most recent timestamp across requirement_changes and price_history for an order.
fn last_activity_ts(conn: &Connection, order_id: i64) -> Result<Option<i64>> {
    let ts: Option<i64> = conn
        .query_row(
            "SELECT MAX(ts) FROM (
                SELECT MAX(created_at) AS ts FROM requirement_changes WHERE order_id = ?1
                UNION ALL
                SELECT MAX(created_at) AS ts FROM price_history WHERE order_id = ?1
             )",
            (order_id,),
            |row| row.get(0),
        )
        .map_err(Error::Db)?;
    Ok(ts)
}

/// Returns (month_start_unix, month_end_unix) for the month containing `now`.
fn current_month_bounds(now: i64) -> (i64, i64) {
    // Use simple arithmetic: extract year/month from unix timestamp.
    // 1970-01-01 is the epoch. We compute day-of-year offset.
    // Simple approach: use divisions.
    let days_since_epoch = now / 86400;
    let (year, month, _) = days_to_ymd(days_since_epoch);

    let month_start = ymd_to_unix(year, month, 1);
    let month_end = if month == 12 {
        ymd_to_unix(year + 1, 1, 1)
    } else {
        ymd_to_unix(year, month + 1, 1)
    };

    (month_start, month_end)
}

/// Convert days-since-epoch to (year, month, day). Gregorian calendar.
fn days_to_ymd(days: i64) -> (i64, i64, i64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Convert (year, month, day) to unix timestamp (seconds since epoch, midnight UTC).
fn ymd_to_unix(year: i64, month: i64, day: i64) -> i64 {
    // Days from epoch to the given date.
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe - 719468) * 86400
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::OrderStatus;
    use crate::repo::orders::{insert, NewOrder};

    fn new_order(status: OrderStatus, created_at: i64) -> NewOrder<'static> {
        NewOrder {
            slug: None,
            external_id: None,
            title: "test",
            client_id: None,
            source_org: None,
            source_id: None,
            project_type: None,
            status,
            quoted_price: Some(10_000),
            final_price: Some(10_000),
            my_cut_ratio: 0.6,
            currency: "CNY",
            notes: None,
            created_at,
            accepted_at: if status != OrderStatus::Lead {
                Some(created_at)
            } else {
                None
            },
        }
    }

    const DAY: i64 = 86400;
    const NOW: i64 = 1_700_000_000;

    #[test]
    fn alert_delivered_unpaid_after_threshold() {
        let conn = open_in_memory().unwrap();
        let mut n = new_order(OrderStatus::Delivered, NOW - 4 * DAY);
        n.status = OrderStatus::Delivered;
        let o = insert(&conn, &n).unwrap();
        // Manually set delivered_at.
        conn.execute(
            "UPDATE orders SET delivered_at = ?1 WHERE id = ?2",
            (NOW - 4 * DAY, o.id),
        )
        .unwrap();

        let alert = compute_alert(
            &crate::repo::orders::find_by_id(&conn, o.id).unwrap(),
            &conn,
            NOW,
        )
        .unwrap();
        assert!(alert.is_some());
        assert!(alert.unwrap().contains("unpaid"));
    }

    #[test]
    fn alert_in_progress_stale_after_threshold() {
        let conn = open_in_memory().unwrap();
        let n = new_order(OrderStatus::InProgress, NOW - 6 * DAY);
        let o = insert(&conn, &n).unwrap();

        let alert = compute_alert(&o, &conn, NOW).unwrap();
        assert!(alert.is_some());
        assert!(alert.unwrap().contains("no updates"));
    }

    #[test]
    fn alert_lead_stale_after_threshold() {
        let conn = open_in_memory().unwrap();
        let n = new_order(OrderStatus::Lead, NOW - 3 * DAY);
        let o = insert(&conn, &n).unwrap();

        let alert = compute_alert(&o, &conn, NOW).unwrap();
        assert!(alert.is_some());
        assert!(alert.unwrap().contains("lead sitting"));
    }

    #[test]
    fn no_alert_for_fresh_lead() {
        let conn = open_in_memory().unwrap();
        let n = new_order(OrderStatus::Lead, NOW - DAY);
        let o = insert(&conn, &n).unwrap();

        let alert = compute_alert(&o, &conn, NOW).unwrap();
        assert!(alert.is_none());
    }

    #[test]
    fn month_bounds_correct() {
        // 2024-01-15 00:00:00 UTC ≈ 1705276800
        let now = 1_705_276_800i64 + 15 * 3600; // middle of day
        let (start, end) = current_month_bounds(now);
        // start should be 2024-01-01 UTC
        let (ys, ms, ds) = days_to_ymd(start / 86400);
        assert_eq!((ys, ms, ds), (2024, 1, 1));
        // end should be 2024-02-01 UTC
        let (ye, me, de) = days_to_ymd(end / 86400);
        assert_eq!((ye, me, de), (2024, 2, 1));
    }
}
