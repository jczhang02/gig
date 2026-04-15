use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Lifecycle status of an order. Serialized as a lowercase string in SQLite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Lead,
    Negotiating,
    Accepted,
    InProgress,
    Delivered,
    Revision,
    Paid,
    Archived,
    Cancelled,
}

impl OrderStatus {
    pub const ALL: &'static [OrderStatus] = &[
        OrderStatus::Lead,
        OrderStatus::Negotiating,
        OrderStatus::Accepted,
        OrderStatus::InProgress,
        OrderStatus::Delivered,
        OrderStatus::Revision,
        OrderStatus::Paid,
        OrderStatus::Archived,
        OrderStatus::Cancelled,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Lead => "lead",
            OrderStatus::Negotiating => "negotiating",
            OrderStatus::Accepted => "accepted",
            OrderStatus::InProgress => "in_progress",
            OrderStatus::Delivered => "delivered",
            OrderStatus::Revision => "revision",
            OrderStatus::Paid => "paid",
            OrderStatus::Archived => "archived",
            OrderStatus::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for OrderStatus {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "lead" => OrderStatus::Lead,
            "negotiating" => OrderStatus::Negotiating,
            "accepted" => OrderStatus::Accepted,
            "in_progress" => OrderStatus::InProgress,
            "delivered" => OrderStatus::Delivered,
            "revision" => OrderStatus::Revision,
            "paid" => OrderStatus::Paid,
            "archived" => OrderStatus::Archived,
            "cancelled" => OrderStatus::Cancelled,
            other => return Err(Error::Invalid(format!("unknown status: {other}"))),
        })
    }
}

/// Order row. Fields mirror `orders` table columns. Timestamps are unix seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Order {
    pub id: i64,
    pub slug: Option<String>,
    pub external_id: Option<String>,
    pub title: String,
    pub client_id: Option<i64>,
    pub source_org: Option<String>,
    pub source_id: Option<i64>,
    pub status: OrderStatus,
    pub quoted_price: Option<i64>,
    pub final_price: Option<i64>,
    pub my_cut_ratio: f64,
    pub currency: String,
    pub dev_path: Option<String>,
    pub archive_path: Option<String>,
    pub notes: Option<String>,
    pub created_at: i64,
    pub accepted_at: Option<i64>,
    pub delivered_at: Option<i64>,
    pub paid_at: Option<i64>,
    pub archived_at: Option<i64>,
}

impl Order {
    /// Computed my_cut amount, rounded to the nearest minor unit.
    /// Returns None when `final_price` is not yet known.
    pub fn my_cut_amount(&self) -> Option<i64> {
        let price = self.final_price?;
        Some((price as f64 * self.my_cut_ratio).round() as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrips_through_str() {
        for s in OrderStatus::ALL {
            let parsed: OrderStatus = s.as_str().parse().unwrap();
            assert_eq!(parsed, *s);
        }
    }

    #[test]
    fn status_from_str_rejects_unknown() {
        assert!("bogus".parse::<OrderStatus>().is_err());
    }

    #[test]
    fn my_cut_amount_is_none_when_final_price_missing() {
        let o = Order {
            id: 1,
            slug: None,
            external_id: None,
            title: "t".into(),
            client_id: None,
            source_org: None,
            source_id: None,
            status: OrderStatus::Accepted,
            quoted_price: None,
            final_price: None,
            my_cut_ratio: 0.6,
            currency: "CNY".into(),
            dev_path: None,
            archive_path: None,
            notes: None,
            created_at: 0,
            accepted_at: None,
            delivered_at: None,
            paid_at: None,
            archived_at: None,
        };
        assert_eq!(o.my_cut_amount(), None);
    }

    #[test]
    fn my_cut_amount_rounds_to_nearest() {
        let o = Order {
            id: 1,
            slug: None,
            external_id: None,
            title: "t".into(),
            client_id: None,
            source_org: None,
            source_id: None,
            status: OrderStatus::Accepted,
            quoted_price: None,
            final_price: Some(10_000),
            my_cut_ratio: 0.6,
            currency: "CNY".into(),
            dev_path: None,
            archive_path: None,
            notes: None,
            created_at: 0,
            accepted_at: None,
            delivered_at: None,
            paid_at: None,
            archived_at: None,
        };
        assert_eq!(o.my_cut_amount(), Some(6_000));
    }
}
