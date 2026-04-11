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
pub fn transition(
    conn: &Connection,
    id: i64,
    to: OrderStatus,
    now: i64,
) -> Result<Order> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
            if matches!(s, OrderStatus::Cancelled | OrderStatus::Archived) { continue; }
            assert!(
                can_transition(s, OrderStatus::Cancelled),
                "{s:?} should be cancellable"
            );
        }
    }

    #[test]
    fn archived_cannot_transition_further() {
        assert!(!can_transition(OrderStatus::Archived, OrderStatus::Paid));
        assert!(!can_transition(OrderStatus::Archived, OrderStatus::Cancelled));
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
}
