use crate::models::{Order, OrderStatus};
use crate::repo::orders as repo;
use crate::{Error, Result};
use rusqlite::Connection;

/// Returns true iff `from → to` is allowed by the v0.1 status machine.
pub fn can_transition(from: OrderStatus, to: OrderStatus) -> bool {
    use OrderStatus::*;
    // Any state can move to cancelled.
    if to == Cancelled && from != Cancelled && from != Archived {
        return true;
    }
    matches!(
        (from, to),
        (Lead, Negotiating)
            | (Lead, Accepted)       // skip negotiating
            | (Negotiating, Accepted)
            | (Accepted, InProgress)
            | (InProgress, InProgress) // change loop: allowed no-op
            | (InProgress, Delivered)
            | (Delivered, Paid)
            | (Paid, Archived)
    )
}

/// Which timestamp column (if any) should be filled when entering `to`.
fn timestamp_col_for(to: OrderStatus) -> Option<&'static str> {
    use OrderStatus::*;
    match to {
        Accepted => Some("accepted_at"),
        Delivered => Some("delivered_at"),
        Paid => Some("paid_at"),
        Archived => Some("archived_at"),
        _ => None,
    }
}

/// Transition an order's status, updating the corresponding timestamp.
/// All transitions flow through this function.
pub fn transition(conn: &Connection, id: i64, to: OrderStatus, now: i64) -> Result<Order> {
    let current = repo::find_by_id(conn, id)?;
    if current.status == to {
        return Ok(current); // idempotent
    }
    if !can_transition(current.status, to) {
        return Err(Error::InvalidTransition {
            from: current.status.to_string(),
            to: to.to_string(),
        });
    }
    let ts_col = timestamp_col_for(to);
    repo::update_status(conn, id, to, ts_col, ts_col.map(|_| now))?;
    repo::find_by_id(conn, id)
}

/// User-supplied fields for creating a new order.
pub struct CreateOrderInput<'a> {
    pub title: &'a str,
    pub slug: Option<&'a str>,
    pub client_id: Option<i64>,
    pub source_org: Option<&'a str>,
    pub quoted_price: Option<i64>,
    pub final_price: Option<i64>,
    pub my_cut_ratio: f64,
    pub currency: &'a str,
    pub notes: Option<&'a str>,
    pub as_lead: bool,
}

/// Create an order. If `as_lead` is true, the order starts in `Lead`;
/// otherwise it starts in `Accepted` with `accepted_at = now`.
pub fn create_order(conn: &Connection, input: &CreateOrderInput<'_>, now: i64) -> Result<Order> {
    validate_create_input(input)?;
    let status = if input.as_lead {
        OrderStatus::Lead
    } else {
        OrderStatus::Accepted
    };
    let accepted_at = (!input.as_lead).then_some(now);
    let new = repo::NewOrder {
        slug: input.slug,
        external_id: None,
        title: input.title,
        client_id: input.client_id,
        source_org: input.source_org,
        status,
        quoted_price: input.quoted_price,
        final_price: input.final_price,
        my_cut_ratio: input.my_cut_ratio,
        currency: input.currency,
        notes: input.notes,
        created_at: now,
        accepted_at,
    };
    repo::insert(conn, &new)
}

fn validate_create_input(input: &CreateOrderInput<'_>) -> Result<()> {
    if input.title.trim().is_empty() {
        return Err(Error::Invalid("title must not be empty".into()));
    }
    if !(0.0..=1.0).contains(&input.my_cut_ratio) {
        return Err(Error::Invalid(format!(
            "my_cut_ratio must be in [0.0, 1.0], got {}",
            input.my_cut_ratio
        )));
    }
    if input.currency.len() != 3 {
        return Err(Error::Invalid(format!(
            "currency must be a 3-letter ISO code, got {:?}",
            input.currency
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn base_input<'a>(title: &'a str, as_lead: bool) -> CreateOrderInput<'a> {
        CreateOrderInput {
            title,
            slug: None,
            client_id: None,
            source_org: None,
            quoted_price: Some(1_000),
            final_price: None,
            my_cut_ratio: 0.6,
            currency: "CNY",
            notes: None,
            as_lead,
        }
    }

    #[test]
    fn lead_to_negotiating_allowed() {
        assert!(can_transition(OrderStatus::Lead, OrderStatus::Negotiating));
    }

    #[test]
    fn lead_to_delivered_forbidden() {
        assert!(!can_transition(OrderStatus::Lead, OrderStatus::Delivered));
    }

    #[test]
    fn any_nonarchived_can_cancel() {
        for &s in OrderStatus::ALL {
            if matches!(s, OrderStatus::Cancelled | OrderStatus::Archived) {
                continue;
            }
            assert!(
                can_transition(s, OrderStatus::Cancelled),
                "{s:?} should be cancellable"
            );
        }
    }

    #[test]
    fn archived_cannot_transition_further() {
        assert!(!can_transition(OrderStatus::Archived, OrderStatus::Paid));
        assert!(!can_transition(
            OrderStatus::Archived,
            OrderStatus::Cancelled
        ));
    }

    #[test]
    fn happy_path_accept_to_paid() {
        let path = &[
            (OrderStatus::Lead, OrderStatus::Negotiating),
            (OrderStatus::Negotiating, OrderStatus::Accepted),
            (OrderStatus::Accepted, OrderStatus::InProgress),
            (OrderStatus::InProgress, OrderStatus::Delivered),
            (OrderStatus::Delivered, OrderStatus::Paid),
            (OrderStatus::Paid, OrderStatus::Archived),
        ];
        for (from, to) in path {
            assert!(can_transition(*from, *to), "{from:?} → {to:?}");
        }
    }

    #[test]
    fn create_order_as_lead_has_no_accepted_at() {
        let conn = open_in_memory().unwrap();
        let o = create_order(&conn, &base_input("t", true), 1_000).unwrap();
        assert_eq!(o.status, OrderStatus::Lead);
        assert_eq!(o.accepted_at, None);
    }

    #[test]
    fn create_order_as_accepted_fills_accepted_at() {
        let conn = open_in_memory().unwrap();
        let o = create_order(&conn, &base_input("t", false), 1_000).unwrap();
        assert_eq!(o.status, OrderStatus::Accepted);
        assert_eq!(o.accepted_at, Some(1_000));
    }

    #[test]
    fn create_order_rejects_empty_title() {
        let conn = open_in_memory().unwrap();
        let mut input = base_input("", false);
        input.title = "   ";
        assert!(matches!(
            create_order(&conn, &input, 0),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn create_order_rejects_bad_ratio() {
        let conn = open_in_memory().unwrap();
        let mut input = base_input("t", false);
        input.my_cut_ratio = 1.5;
        assert!(matches!(
            create_order(&conn, &input, 0),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn create_order_rejects_bad_currency() {
        let conn = open_in_memory().unwrap();
        let mut input = base_input("t", false);
        input.currency = "YUAN";
        assert!(matches!(
            create_order(&conn, &input, 0),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn transition_from_accepted_to_in_progress_round_trips() {
        let conn = open_in_memory().unwrap();
        let o = create_order(&conn, &base_input("t", false), 500).unwrap();
        let moved = transition(&conn, o.id, OrderStatus::InProgress, 600).unwrap();
        assert_eq!(moved.status, OrderStatus::InProgress);
        // no timestamp column for InProgress
        assert_eq!(moved.accepted_at, Some(500));
    }

    #[test]
    fn transition_invalid_returns_error() {
        let conn = open_in_memory().unwrap();
        let o = create_order(&conn, &base_input("t", true), 0).unwrap();
        match transition(&conn, o.id, OrderStatus::Delivered, 1) {
            Err(Error::InvalidTransition { .. }) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}
