//! Money view model (spec section 2.3). All amounts are minor units; each
//! total carries its take-home, summed per order as round(price x cut_ratio)
//! because the cut differs between orders.

use super::{calendar_days, day_part};
use gig_core::models::{Order, OrderStatus};

/// Months in the bar chart.
pub const MONTHS: usize = 12;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Amount {
    pub gross: i64,
    pub take_home: i64,
}

impl Amount {
    fn add(&mut self, o: &Order) {
        let price = o.price_minor.unwrap_or(0);
        self.gross += price;
        self.take_home += take_home(price, o.cut_ratio);
    }
}

pub fn take_home(price_minor: i64, cut_ratio: f64) -> i64 {
    (price_minor as f64 * cut_ratio).round() as i64
}

/// One delivered, unpaid order.
#[derive(Debug, Clone, PartialEq)]
pub struct Owed {
    pub order_id: i64,
    pub slug: String,
    pub price_minor: Option<i64>,
    pub currency: String,
    /// Calendar days since `delivered_at`; `None` when it is missing.
    pub days: Option<i64>,
}

/// Received in one calendar month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Month {
    /// `YYYY-MM`.
    pub label: String,
    pub amount: Amount,
    /// The orders paid in the month, newest payment first (the Money
    /// drill-down, TUI-SPEC 8.2).
    pub order_ids: Vec<i64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Money {
    /// Delivered and unpaid.
    pub outstanding: Amount,
    /// Paid with `paid_at` in the current month / year.
    pub month: Amount,
    pub year: Amount,
    /// The last `MONTHS` months, oldest first, current month last; months
    /// without payments are present with zero.
    pub by_month: Vec<Month>,
    /// Outstanding orders, longest waiting first.
    pub owed: Vec<Owed>,
}

impl Money {
    /// Aggregate over all orders. `today` is `YYYY-MM-DD`.
    ///
    /// Received money is every order with a `paid_at` (paid or archived),
    /// keyed by the date part of `paid_at` (a date from `gig paid`, or an
    /// RFC 3339 instant from migrated rows). Cancelled orders are left out
    /// even when they carry a `paid_at`.
    pub fn compute(orders: &[Order], today: &str) -> Self {
        let this_month = today.get(..7).unwrap_or("");
        let this_year = today.get(..4).unwrap_or("");
        let labels = last_months(today, MONTHS);
        let mut by_month: Vec<Month> = labels
            .into_iter()
            .map(|label| Month {
                label,
                amount: Amount::default(),
                order_ids: Vec::new(),
            })
            .collect();
        let mut m = Money::default();

        for o in orders {
            if o.status == OrderStatus::Delivered {
                m.outstanding.add(o);
                m.owed.push(Owed {
                    order_id: o.id,
                    slug: o.slug.clone(),
                    price_minor: o.price_minor,
                    currency: o.currency.clone(),
                    days: o
                        .delivered_at
                        .as_deref()
                        .and_then(|d| calendar_days(d, today)),
                });
            }
            if o.status == OrderStatus::Cancelled {
                continue;
            }
            let Some(paid) = o.paid_at.as_deref().and_then(day_part) else {
                continue;
            };
            if paid.get(..4) == Some(this_year) {
                m.year.add(o);
            }
            if paid.get(..7) == Some(this_month) {
                m.month.add(o);
            }
            if let Some(slot) = by_month
                .iter_mut()
                .find(|s| paid.get(..7) == Some(s.label.as_str()))
            {
                slot.amount.add(o);
                slot.order_ids.push(o.id);
            }
        }
        // Newest payment first; the same day by id, newest first.
        let paid_on = |id: &i64| {
            orders
                .iter()
                .find(|o| o.id == *id)
                .and_then(|o| o.paid_at.as_deref())
                .and_then(day_part)
                .map(str::to_string)
        };
        for slot in &mut by_month {
            slot.order_ids
                .sort_by(|a, b| paid_on(b).cmp(&paid_on(a)).then(b.cmp(a)));
        }
        m.owed.sort_by(|a, b| {
            b.days
                .unwrap_or(i64::MIN)
                .cmp(&a.days.unwrap_or(i64::MIN))
                .then(b.order_id.cmp(&a.order_id))
        });
        m.by_month = by_month;
        m
    }
}

/// `YYYY-MM` labels of the `n` months ending with the month of `today`,
/// oldest first. Empty when `today` is not a date.
pub fn last_months(today: &str, n: usize) -> Vec<String> {
    let parsed = today
        .get(..4)
        .and_then(|y| y.parse::<i32>().ok())
        .zip(today.get(5..7).and_then(|m| m.parse::<i32>().ok()));
    let Some((year, month)) = parsed.filter(|(_, m)| (1..=12).contains(m)) else {
        return Vec::new();
    };
    let current = year * 12 + (month - 1);
    (0..n as i32)
        .rev()
        .map(|back| {
            let k = current - back;
            format!("{:04}-{:02}", k.div_euclid(12), k.rem_euclid(12) + 1)
        })
        .collect()
}

/// Minor units as major units for tight spots: "800", "800.50", "-20".
pub fn major(minor: i64) -> String {
    let s = gig_core::money::format_minor(minor);
    s.strip_suffix(".00").map(str::to_string).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::super::tests::order;
    use super::*;

    #[test]
    fn month_labels_cross_year() {
        let m = last_months("2026-03-15", 12);
        assert_eq!(m.len(), 12);
        assert_eq!(m.first().unwrap(), "2025-04");
        assert_eq!(m.last().unwrap(), "2026-03");
        assert_eq!(last_months("2026-01-01", 2), vec!["2025-12", "2026-01"]);
        assert!(last_months("garbage", 12).is_empty());
    }

    #[test]
    fn major_units() {
        assert_eq!(major(80000), "800");
        assert_eq!(major(80050), "800.50");
        assert_eq!(major(0), "0");
    }

    #[test]
    fn take_home_rounds_per_order() {
        assert_eq!(take_home(80000, 0.6), 48000);
        assert_eq!(take_home(333, 0.5), 167);
    }

    #[test]
    fn aggregates_outstanding_and_received() {
        let today = "2026-09-29";
        let mut orders = Vec::new();

        // Delivered 10 days ago, unpaid.
        let mut a = order(1, OrderStatus::Delivered, 80000);
        a.delivered_at = Some("2026-09-19T08:00:00Z".into());
        orders.push(a);
        // Delivered 40 days ago, unpaid, other cut.
        let mut b = order(2, OrderStatus::Delivered, 50000);
        b.cut_ratio = 1.0;
        b.delivered_at = Some("2026-08-20T08:00:00Z".into());
        orders.push(b);
        // Paid this month (date form).
        let mut c = order(3, OrderStatus::Paid, 100000);
        c.paid_at = Some("2026-09-02".into());
        orders.push(c);
        // Archived, paid in March (migrated RFC 3339 form).
        let mut d = order(4, OrderStatus::Archived, 20000);
        d.cut_ratio = 0.5;
        d.paid_at = Some("2026-03-10T12:00:00Z".into());
        orders.push(d);
        // Paid last year, 13 months back: counted nowhere but history.
        let mut e = order(5, OrderStatus::Archived, 99900);
        e.paid_at = Some("2025-08-31".into());
        orders.push(e);
        // Paid last October: in the chart, not in this year.
        let mut f = order(6, OrderStatus::Archived, 30000);
        f.paid_at = Some("2025-10-01".into());
        orders.push(f);
        // Cancelled with a paid_at: ignored.
        let mut g = order(7, OrderStatus::Cancelled, 70000);
        g.paid_at = Some("2026-09-05".into());
        orders.push(g);
        // Queued and in progress: ignored.
        orders.push(order(8, OrderStatus::Queued, 10000));
        orders.push(order(9, OrderStatus::InProgress, 10000));

        let m = Money::compute(&orders, today);
        assert_eq!(
            m.outstanding,
            Amount {
                gross: 130000,
                take_home: 48000 + 50000
            }
        );
        assert_eq!(
            m.month,
            Amount {
                gross: 100000,
                take_home: 60000
            }
        );
        assert_eq!(
            m.year,
            Amount {
                gross: 120000,
                take_home: 70000
            }
        );
        assert_eq!(m.by_month.len(), 12);
        assert_eq!(m.by_month[0].label, "2025-10");
        assert_eq!(m.by_month[0].amount.gross, 30000);
        let march = m.by_month.iter().find(|x| x.label == "2026-03").unwrap();
        assert_eq!(march.amount.gross, 20000);
        assert_eq!(m.by_month[11].label, "2026-09");
        assert_eq!(m.by_month[11].amount.gross, 100000);
        // The drill-down: the orders paid in a month, cancelled ones left out.
        assert_eq!(m.by_month[11].order_ids, vec![3]);
        assert_eq!(march.order_ids, vec![4]);
        assert!(m.by_month[1].order_ids.is_empty());
        let total: i64 = m.by_month.iter().map(|x| x.amount.gross).sum();
        assert_eq!(total, 150000);

        let owed: Vec<_> = m.owed.iter().map(|o| (o.order_id, o.days)).collect();
        assert_eq!(owed, vec![(2, Some(40)), (1, Some(10))]);
    }

    #[test]
    fn month_orders_newest_payment_first() {
        let mut a = order(1, OrderStatus::Paid, 1000);
        a.paid_at = Some("2026-09-20".into());
        let mut b = order(2, OrderStatus::Archived, 1000);
        b.paid_at = Some("2026-09-02T10:00:00Z".into());
        let mut c = order(3, OrderStatus::Paid, 1000);
        c.paid_at = Some("2026-09-25".into());
        let m = Money::compute(&[a, b, c], "2026-09-29");
        assert_eq!(m.by_month[11].order_ids, vec![3, 1, 2]);
    }

    #[test]
    fn empty_is_zero() {
        let m = Money::compute(&[], "2026-09-29");
        assert_eq!(m.outstanding, Amount::default());
        assert_eq!(m.by_month.len(), 12);
        assert!(m.owed.is_empty());
    }
}
