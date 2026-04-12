//! Stats service: compute income/order statistics for a date range.

use crate::{Error, Result};
use rusqlite::{params, Connection};

/// How to group the breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    Tag,
    Client,
}

/// Computed stats for a date range.
#[derive(Debug, Clone)]
pub struct Stats {
    /// Sum of my_cut_amount for paid orders in range (minor units).
    pub total_income: i64,
    pub order_count: usize,
    pub avg_order_value: Option<i64>,
    /// lead-to-accepted conversion rate (accepted / (accepted + lead + negotiating)).
    pub lead_to_accepted_rate: Option<f64>,
    /// Breakdown by group: (group_name, income, count).
    pub breakdown: Vec<(String, i64, usize)>,
}

/// Compute stats. `range` is optional `(start_unix, end_unix)`.
pub fn compute_stats(
    conn: &Connection,
    range: Option<(i64, i64)>,
    group_by: Option<GroupBy>,
) -> Result<Stats> {
    let (start, end) = range.unwrap_or_else(|| current_month_bounds(now_unix()));

    // Paid orders in range.
    let paid_orders: Vec<(i64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT id, CAST(ROUND(COALESCE(final_price,0) * my_cut_ratio) AS INTEGER) AS cut
             FROM orders
             WHERE status = 'paid' AND paid_at >= ?1 AND paid_at < ?2",
        )?;
        let rows = stmt
            .query_map(params![start, end], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::Db)?;
        rows
    };

    let total_income: i64 = paid_orders.iter().map(|(_, c)| c).sum();
    let order_count = paid_orders.len();
    let avg_order_value = if order_count > 0 {
        Some(total_income / order_count as i64)
    } else {
        None
    };

    // Funnel: count all orders ever created (not filtered by date for funnel).
    let lead_to_accepted_rate = compute_funnel(conn)?;

    // Breakdown.
    let breakdown = match group_by {
        None => vec![],
        Some(GroupBy::Tag) => breakdown_by_tag(conn, start, end)?,
        Some(GroupBy::Client) => breakdown_by_client(conn, start, end)?,
    };

    Ok(Stats {
        total_income,
        order_count,
        avg_order_value,
        lead_to_accepted_rate,
        breakdown,
    })
}

fn compute_funnel(conn: &Connection) -> Result<Option<f64>> {
    let accepted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM orders WHERE status NOT IN ('lead', 'negotiating', 'cancelled')",
            [],
            |r| r.get(0),
        )
        .map_err(Error::Db)?;
    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM orders WHERE status != 'cancelled'",
            [],
            |r| r.get(0),
        )
        .map_err(Error::Db)?;
    if total == 0 {
        Ok(None)
    } else {
        Ok(Some(accepted as f64 / total as f64))
    }
}

fn breakdown_by_tag(conn: &Connection, start: i64, end: i64) -> Result<Vec<(String, i64, usize)>> {
    let mut stmt = conn.prepare(
        "SELECT t.name,
                COALESCE(SUM(CAST(ROUND(o.my_cut_ratio * COALESCE(o.final_price, 0)) AS INTEGER)), 0),
                COUNT(o.id)
         FROM tags t
         JOIN order_tags ot ON ot.tag_id = t.id
         JOIN orders o ON o.id = ot.order_id
         WHERE o.status = 'paid' AND o.paid_at >= ?1 AND o.paid_at < ?2
         GROUP BY t.id, t.name
         ORDER BY 2 DESC",
    )?;
    let rows = stmt
        .query_map(params![start, end], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)? as usize,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::Db)?;
    Ok(rows)
}

fn breakdown_by_client(
    conn: &Connection,
    start: i64,
    end: i64,
) -> Result<Vec<(String, i64, usize)>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(c.display_name, '(no client)'),
                COALESCE(SUM(CAST(ROUND(o.my_cut_ratio * COALESCE(o.final_price, 0)) AS INTEGER)), 0),
                COUNT(o.id)
         FROM orders o
         LEFT JOIN clients c ON c.id = o.client_id
         WHERE o.status = 'paid' AND o.paid_at >= ?1 AND o.paid_at < ?2
         GROUP BY o.client_id
         ORDER BY 2 DESC",
    )?;
    let rows = stmt
        .query_map(params![start, end], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)? as usize,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::Db)?;
    Ok(rows)
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Returns (month_start, month_end) unix timestamps for the month containing `now`.
pub fn current_month_bounds(now: i64) -> (i64, i64) {
    let days = now / 86400;
    let (year, month, _) = days_to_ymd(days);
    let start = ymd_to_unix(year, month, 1);
    let end = if month == 12 {
        ymd_to_unix(year + 1, 1, 1)
    } else {
        ymd_to_unix(year, month + 1, 1)
    };
    (start, end)
}

/// Parse "YYYY-MM" into a unix timestamp (start of month UTC).
pub fn parse_ym(s: &str) -> Option<i64> {
    let (y, m) = s.split_once('-')?;
    let year: i64 = y.parse().ok()?;
    let month: i64 = m.parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    Some(ymd_to_unix(year, month, 1))
}

/// Parse "YYYY-MM..YYYY-MM" range into (start, end) unix timestamps.
/// End is the start of the next month after the end month.
pub fn parse_month_range(s: &str) -> Option<(i64, i64)> {
    let (start_s, end_s) = s.split_once("..")?;
    let start = parse_ym(start_s.trim())?;
    let end_month_start = parse_ym(end_s.trim())?;
    // end = start of month AFTER end_s.
    let (y, m, _) = days_to_ymd(end_month_start / 86400);
    let end = if m == 12 {
        ymd_to_unix(y + 1, 1, 1)
    } else {
        ymd_to_unix(y, m + 1, 1)
    };
    Some((start, end))
}

fn days_to_ymd(days: i64) -> (i64, i64, i64) {
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

fn ymd_to_unix(year: i64, month: i64, day: i64) -> i64 {
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

    fn paid_order(cut: i64, paid_at: i64) -> NewOrder<'static> {
        NewOrder {
            slug: None,
            external_id: None,
            title: "t",
            client_id: None,
            source_org: None,
            status: OrderStatus::Paid,
            quoted_price: Some(cut),
            final_price: Some(cut),
            my_cut_ratio: 1.0,
            currency: "CNY",
            notes: None,
            created_at: paid_at,
            accepted_at: Some(paid_at),
        }
    }

    #[test]
    fn stats_totals_this_month() {
        let conn = open_in_memory().unwrap();
        // 2024-01-15 UTC
        let now = 1_705_276_800i64;
        let (start, _end) = current_month_bounds(now);
        let paid_at = start + 86400; // one day into the month

        let mut n = paid_order(10_000, paid_at);
        n.status = OrderStatus::Paid;
        let o = insert(&conn, &n).unwrap();
        conn.execute(
            "UPDATE orders SET paid_at = ?1 WHERE id = ?2",
            rusqlite::params![paid_at, o.id],
        )
        .unwrap();

        let s = compute_stats(&conn, Some((start, start + 30 * 86400)), None).unwrap();
        assert_eq!(s.total_income, 10_000);
        assert_eq!(s.order_count, 1);
        assert_eq!(s.avg_order_value, Some(10_000));
    }

    #[test]
    fn parse_month_range_valid() {
        let r = parse_month_range("2024-01..2024-03").unwrap();
        // start = 2024-01-01
        let (ys, ms, ds) = days_to_ymd(r.0 / 86400);
        assert_eq!((ys, ms, ds), (2024, 1, 1));
        // end = 2024-04-01 (month after March)
        let (ye, me, de) = days_to_ymd(r.1 / 86400);
        assert_eq!((ye, me, de), (2024, 4, 1));
    }
}
