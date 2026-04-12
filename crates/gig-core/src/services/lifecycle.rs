//! Lifecycle mutation services: price, change, notes, tags, cut, paid, archive, lead.

use crate::models::{Order, OrderStatus, PriceHistoryEntry, RequirementChange, Tag};
use crate::repo::{orders as order_repo, price_history, requirement_changes, tags as tag_repo};
use crate::services::orders::transition;
use crate::{Error, Result};
use rusqlite::Connection;
use std::path::Path;

// ─── Price ────────────────────────────────────────────────────────────────────

/// Update `final_price` and record the change in price_history.
pub fn change_price(
    conn: &Connection,
    id: i64,
    new_price: i64,
    reason: Option<&str>,
    now: i64,
) -> Result<Order> {
    let order = order_repo::find_by_id(conn, id)?;
    let old_price = order.final_price;
    price_history::record(conn, id, old_price, Some(new_price), reason, now)?;
    order_repo::update_price(conn, id, Some(new_price))?;
    order_repo::find_by_id(conn, id)
}

// ─── Requirement change ───────────────────────────────────────────────────────

/// Record a requirement change. If `price_delta != 0`, adjusts `final_price` too.
pub fn add_requirement_change(
    conn: &Connection,
    id: i64,
    description: &str,
    price_delta: i64,
    now: i64,
) -> Result<Order> {
    requirement_changes::add(conn, id, description, price_delta, now)?;
    if price_delta != 0 {
        let order = order_repo::find_by_id(conn, id)?;
        let new_price = order.final_price.unwrap_or(0) + price_delta;
        order_repo::update_price(conn, id, Some(new_price))?;
    }
    order_repo::find_by_id(conn, id)
}

// ─── Notes ────────────────────────────────────────────────────────────────────

/// Append text to existing notes (separated by a newline), or set if empty.
pub fn append_notes(conn: &Connection, id: i64, text: &str) -> Result<Order> {
    let order = order_repo::find_by_id(conn, id)?;
    let new_notes = match order.notes.as_deref() {
        None | Some("") => text.to_string(),
        Some(existing) => format!("{existing}\n{text}"),
    };
    order_repo::update_notes(conn, id, Some(&new_notes))?;
    order_repo::find_by_id(conn, id)
}

// ─── Cut ratio ────────────────────────────────────────────────────────────────

/// Update the cut ratio. Must be in [0.0, 1.0].
pub fn update_cut_ratio(conn: &Connection, id: i64, ratio: f64) -> Result<Order> {
    if !(0.0..=1.0).contains(&ratio) {
        return Err(Error::Invalid(format!(
            "cut ratio must be in [0.0, 1.0], got {ratio}"
        )));
    }
    order_repo::update_cut_ratio(conn, id, ratio)?;
    order_repo::find_by_id(conn, id)
}

// ─── Tags ─────────────────────────────────────────────────────────────────────

/// Add one or more tags to an order. Creates the tag if it does not exist.
pub fn add_tags(conn: &Connection, order_id: i64, tag_names: &[&str]) -> Result<Vec<Tag>> {
    // Verify order exists
    order_repo::find_by_id(conn, order_id)?;
    for name in tag_names {
        let tag = tag_repo::find_or_create(conn, name)?;
        tag_repo::attach(conn, order_id, tag.id)?;
    }
    tag_repo::list_for_order(conn, order_id)
}

/// Remove a single tag from an order by name.
pub fn remove_tag(conn: &Connection, order_id: i64, tag_name: &str) -> Result<Vec<Tag>> {
    order_repo::find_by_id(conn, order_id)?;
    tag_repo::detach_by_name(conn, order_id, tag_name)?;
    tag_repo::list_for_order(conn, order_id)
}

/// List all tags for an order.
pub fn list_tags(conn: &Connection, order_id: i64) -> Result<Vec<Tag>> {
    order_repo::find_by_id(conn, order_id)?;
    tag_repo::list_for_order(conn, order_id)
}

// ─── Paid ─────────────────────────────────────────────────────────────────────

/// Transition order to Paid and record paid_at timestamp.
pub fn mark_paid(conn: &Connection, id: i64, paid_at: i64) -> Result<Order> {
    transition(conn, id, OrderStatus::Paid, paid_at)
}

// ─── Archive ──────────────────────────────────────────────────────────────────

/// Transition order to Archived. If it has a dev_path, move the folder to
/// `<archive_root>/<slug>/`, canonicalize, store archive_path, clear dev_path.
pub fn archive_order(conn: &Connection, id: i64, archive_root: &Path, now: i64) -> Result<Order> {
    let order = order_repo::find_by_id(conn, id)?;

    if let Some(ref dev_path) = order.dev_path {
        let src = Path::new(dev_path);
        let slug = order
            .slug
            .as_deref()
            .ok_or_else(|| Error::Invalid("order has no slug — cannot archive".into()))?;
        let dest = archive_root.join(slug);
        std::fs::create_dir_all(archive_root)
            .map_err(|e| Error::PathUnavailable(archive_root.to_path_buf(), e))?;
        // Try rename first; fall back to recursive copy+delete on cross-device moves.
        match std::fs::rename(src, &dest) {
            Ok(()) => {}
            Err(e) if e.raw_os_error() == Some(18) => {
                // Error 18 = EXDEV: cross-device link not permitted.
                copy_dir_recursive(src, &dest)
                    .map_err(|e2| Error::PathUnavailable(dest.clone(), e2))?;
                std::fs::remove_dir_all(src).map_err(Error::Io)?;
            }
            Err(e) => return Err(Error::PathUnavailable(dest.clone(), e)),
        }
        let canonical =
            std::fs::canonicalize(&dest).map_err(|e| Error::PathUnavailable(dest.clone(), e))?;
        let archive_path_str = canonical
            .to_str()
            .ok_or_else(|| Error::Invalid("archive path is not valid UTF-8".into()))?
            .to_string();
        order_repo::update_archive_path(conn, id, Some(&archive_path_str))?;
    }

    transition(conn, id, OrderStatus::Archived, now)
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Recursively copy a directory tree from `src` to `dst`.
/// Used as a fallback when `std::fs::rename` fails across filesystems (EXDEV).
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

// ─── Lead ─────────────────────────────────────────────────────────────────────

/// Promote a lead to Negotiating.
pub fn promote_lead(conn: &Connection, id: i64, now: i64) -> Result<Order> {
    transition(conn, id, OrderStatus::Negotiating, now)
}

/// Drop a lead to Cancelled.
pub fn drop_lead(conn: &Connection, id: i64, now: i64) -> Result<Order> {
    transition(conn, id, OrderStatus::Cancelled, now)
}

// ─── Status override ──────────────────────────────────────────────────────────

/// Force-transition to any status (via the normal state-machine rules).
pub fn set_status(conn: &Connection, id: i64, status: OrderStatus, now: i64) -> Result<Order> {
    transition(conn, id, status, now)
}

// ─── Read helpers ─────────────────────────────────────────────────────────────

pub fn get_price_history(conn: &Connection, order_id: i64) -> Result<Vec<PriceHistoryEntry>> {
    price_history::list_for_order(conn, order_id)
}

pub fn get_requirement_changes(conn: &Connection, order_id: i64) -> Result<Vec<RequirementChange>> {
    requirement_changes::list_for_order(conn, order_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::OrderStatus;
    use crate::services::orders::{create_order, transition as svc_transition, CreateOrderInput};
    use tempfile::TempDir;

    fn make_order(conn: &Connection, as_lead: bool) -> Order {
        create_order(
            conn,
            &CreateOrderInput {
                title: "test order",
                slug: Some("test-order"),
                client_id: None,
                source_org: None,
                source_id: None,
                quoted_price: Some(10_000),
                final_price: Some(10_000),
                my_cut_ratio: 0.6,
                currency: "CNY",
                notes: None,
                as_lead,
            },
            1_000,
        )
        .unwrap()
    }

    fn accepted_order(conn: &Connection) -> Order {
        make_order(conn, false)
    }

    fn lead_order(conn: &Connection) -> Order {
        make_order(conn, true)
    }

    // ── change_price ──

    #[test]
    fn change_price_updates_final_price() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        let updated = change_price(&conn, o.id, 20_000, Some("scope expanded"), 2_000).unwrap();
        assert_eq!(updated.final_price, Some(20_000));
    }

    #[test]
    fn change_price_records_history() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        change_price(&conn, o.id, 15_000, None, 2_000).unwrap();
        let history = get_price_history(&conn, o.id).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].old_price, Some(10_000));
        assert_eq!(history[0].new_price, Some(15_000));
    }

    #[test]
    fn change_price_order_not_found() {
        let conn = open_in_memory().unwrap();
        assert!(change_price(&conn, 9999, 100, None, 0).is_err());
    }

    // ── add_requirement_change ──

    #[test]
    fn requirement_change_adjusts_price() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        let updated = add_requirement_change(&conn, o.id, "extra feature", 5_000, 2_000).unwrap();
        assert_eq!(updated.final_price, Some(15_000));
    }

    #[test]
    fn requirement_change_zero_delta_no_price_change() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        let updated = add_requirement_change(&conn, o.id, "just a note", 0, 2_000).unwrap();
        assert_eq!(updated.final_price, Some(10_000));
    }

    // ── append_notes ──

    #[test]
    fn append_notes_sets_when_empty() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        let updated = append_notes(&conn, o.id, "first note").unwrap();
        assert_eq!(updated.notes.as_deref(), Some("first note"));
    }

    #[test]
    fn append_notes_appends_to_existing() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        append_notes(&conn, o.id, "line one").unwrap();
        let updated = append_notes(&conn, o.id, "line two").unwrap();
        assert!(updated.notes.as_deref().unwrap().contains("line one"));
        assert!(updated.notes.as_deref().unwrap().contains("line two"));
    }

    // ── update_cut_ratio ──

    #[test]
    fn cut_ratio_updates() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        let updated = update_cut_ratio(&conn, o.id, 0.75).unwrap();
        assert!((updated.my_cut_ratio - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn cut_ratio_rejects_out_of_range() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        assert!(update_cut_ratio(&conn, o.id, 1.5).is_err());
        assert!(update_cut_ratio(&conn, o.id, -0.1).is_err());
    }

    // ── tags ──

    #[test]
    fn add_and_list_tags() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        add_tags(&conn, o.id, &["python", "scraper"]).unwrap();
        let tags = list_tags(&conn, o.id).unwrap();
        assert_eq!(tags.len(), 2);
    }

    #[test]
    fn remove_tag_works() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        add_tags(&conn, o.id, &["python", "web"]).unwrap();
        let remaining = remove_tag(&conn, o.id, "python").unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "web");
    }

    // ── mark_paid ──

    #[test]
    fn mark_paid_transitions_to_paid() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        svc_transition(&conn, o.id, OrderStatus::InProgress, 2_000).unwrap();
        svc_transition(&conn, o.id, OrderStatus::Delivered, 3_000).unwrap();
        let paid = mark_paid(&conn, o.id, 4_000).unwrap();
        assert_eq!(paid.status, OrderStatus::Paid);
        assert_eq!(paid.paid_at, Some(4_000));
    }

    #[test]
    fn mark_paid_from_wrong_status_fails() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        assert!(mark_paid(&conn, o.id, 1_000).is_err());
    }

    // ── archive_order ──

    #[test]
    fn archive_without_dev_path_only_transitions() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn);
        svc_transition(&conn, o.id, OrderStatus::InProgress, 2_000).unwrap();
        svc_transition(&conn, o.id, OrderStatus::Delivered, 3_000).unwrap();
        svc_transition(&conn, o.id, OrderStatus::Paid, 4_000).unwrap();
        let tmp = TempDir::new().unwrap();
        let archived = archive_order(&conn, o.id, tmp.path(), 5_000).unwrap();
        assert_eq!(archived.status, OrderStatus::Archived);
        assert_eq!(archived.archive_path, None);
    }

    // ── promote_lead / drop_lead ──

    #[test]
    fn promote_lead_to_negotiating() {
        let conn = open_in_memory().unwrap();
        let o = lead_order(&conn);
        let promoted = promote_lead(&conn, o.id, 2_000).unwrap();
        assert_eq!(promoted.status, OrderStatus::Negotiating);
    }

    #[test]
    fn drop_lead_to_cancelled() {
        let conn = open_in_memory().unwrap();
        let o = lead_order(&conn);
        let dropped = drop_lead(&conn, o.id, 2_000).unwrap();
        assert_eq!(dropped.status, OrderStatus::Cancelled);
    }

    #[test]
    fn promote_lead_rejects_non_lead() {
        let conn = open_in_memory().unwrap();
        let o = accepted_order(&conn); // accepted, not lead
        assert!(promote_lead(&conn, o.id, 2_000).is_err());
    }
}
