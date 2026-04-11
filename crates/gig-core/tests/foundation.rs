//! End-to-end integration test for Plan 1 foundation.
//!
//! Exercises open → create → list → find for a fresh temp database,
//! proving the crates wire together.

use gig_core::db;
use gig_core::models::OrderStatus;
use gig_core::repo::orders::{find_by_id, find_by_id_or_slug, list, ListFilter};
use gig_core::services::orders::{create_order, transition, CreateOrderInput};
use tempfile::TempDir;

fn new_input<'a>(title: &'a str, slug: Option<&'a str>, lead: bool) -> CreateOrderInput<'a> {
    CreateOrderInput {
        title,
        slug,
        client_id: None,
        source_org: None,
        quoted_price: Some(100_000),
        final_price: None,
        my_cut_ratio: 0.6,
        currency: "CNY",
        notes: None,
        as_lead: lead,
    }
}

#[test]
fn fresh_db_round_trip_create_list_show_transition() {
    let tmp = TempDir::new().unwrap();
    let db_file = tmp.path().join("gig.db");
    let conn = db::open(&db_file).unwrap();

    // create a lead and a full order
    let lead = create_order(&conn, &new_input("potential scrape job", None, true), 1_000).unwrap();
    let full = create_order(
        &conn,
        &new_input("dashboard", Some("dashboard"), false),
        2_000,
    )
    .unwrap();

    assert_eq!(lead.status, OrderStatus::Lead);
    assert_eq!(full.status, OrderStatus::Accepted);
    assert_eq!(full.accepted_at, Some(2_000));

    // list all
    let all = list(&conn, &ListFilter { status: None }).unwrap();
    assert_eq!(all.len(), 2);

    // list only leads
    let leads = list(
        &conn,
        &ListFilter {
            status: Some(OrderStatus::Lead),
        },
    )
    .unwrap();
    assert_eq!(leads.len(), 1);
    assert_eq!(leads[0].id, lead.id);

    // find by slug
    let by_slug = find_by_id_or_slug(&conn, "dashboard").unwrap();
    assert_eq!(by_slug.id, full.id);

    // transition happy path
    let moved = transition(&conn, full.id, OrderStatus::InProgress, 3_000).unwrap();
    assert_eq!(moved.status, OrderStatus::InProgress);
    assert_eq!(
        find_by_id(&conn, full.id).unwrap().status,
        OrderStatus::InProgress
    );

    // rejects illegal transitions
    let err = transition(&conn, lead.id, OrderStatus::Paid, 4_000).unwrap_err();
    assert!(err.to_string().contains("invalid status transition"));
}
