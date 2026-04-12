//! End-to-end lifecycle integration test.
//!
//! Exercises: create → init → price change → requirement change → tag → paid → archive,
//! verifying state at each step.

use gig_core::db;
use gig_core::models::OrderStatus;
use gig_core::services::{
    init::init_project,
    lifecycle::{
        add_requirement_change, add_tags, archive_order, change_price, get_price_history,
        get_requirement_changes, list_tags, mark_paid,
    },
    orders::{create_order, transition, CreateOrderInput},
};
use tempfile::TempDir;

fn make_conn(tmp: &TempDir) -> rusqlite::Connection {
    let db_file = tmp.path().join("gig.db");
    db::open(&db_file).unwrap()
}

fn accepted_input<'a>(title: &'a str, slug: &'a str) -> CreateOrderInput<'a> {
    CreateOrderInput {
        title,
        slug: Some(slug),
        client_id: None,
        source_org: Some("Acme Corp"),
        quoted_price: Some(50_000),
        final_price: Some(50_000),
        my_cut_ratio: 0.6,
        currency: "CNY",
        notes: Some("initial notes"),
        as_lead: false,
    }
}

#[test]
fn full_lifecycle_create_to_archive() {
    let tmp = TempDir::new().unwrap();
    let conn = make_conn(&tmp);

    // 1. Create accepted order
    let order = create_order(
        &conn,
        &accepted_input("Acme Scraper", "acme-scraper"),
        1_000,
    )
    .unwrap();
    assert_eq!(order.status, OrderStatus::Accepted);
    assert_eq!(order.slug.as_deref(), Some("acme-scraper"));
    assert_eq!(order.accepted_at, Some(1_000));

    // 2. Init → creates project folder, transitions to in_progress
    let dev_root = tmp.path().join("dev");
    std::fs::create_dir_all(&dev_root).unwrap();
    let order = init_project(&conn, order.id, &dev_root, None).unwrap();
    assert_eq!(order.status, OrderStatus::InProgress);
    let dev_path = order.dev_path.as_deref().expect("dev_path must be set");
    assert!(std::path::Path::new(dev_path).exists());
    assert!(std::path::Path::new(dev_path).join("README.md").exists());
    assert!(std::path::Path::new(dev_path).join(".gitignore").exists());

    // 3. Price change
    let order = change_price(&conn, order.id, 60_000, Some("scope expanded"), 2_000).unwrap();
    assert_eq!(order.final_price, Some(60_000));
    let history = get_price_history(&conn, order.id).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].old_price, Some(50_000));
    assert_eq!(history[0].new_price, Some(60_000));
    assert_eq!(history[0].reason.as_deref(), Some("scope expanded"));

    // 4. Requirement change (with delta)
    let order =
        add_requirement_change(&conn, order.id, "add export feature", 5_000, 3_000).unwrap();
    assert_eq!(order.final_price, Some(65_000));
    let changes = get_requirement_changes(&conn, order.id).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].description, "add export feature");
    assert_eq!(changes[0].price_delta, 5_000);

    // 5. Add tags
    let tags = add_tags(&conn, order.id, &["python", "scraper", "web"]).unwrap();
    assert_eq!(tags.len(), 3);
    let fetched_tags = list_tags(&conn, order.id).unwrap();
    assert_eq!(fetched_tags.len(), 3);

    // 6. Deliver → Paid
    let order = transition(&conn, order.id, OrderStatus::Delivered, 4_000).unwrap();
    assert_eq!(order.status, OrderStatus::Delivered);

    let order = mark_paid(&conn, order.id, 5_000).unwrap();
    assert_eq!(order.status, OrderStatus::Paid);
    assert_eq!(order.paid_at, Some(5_000));

    // 7. Archive → moves folder
    let archive_root = tmp.path().join("archive");
    let order = archive_order(&conn, order.id, &archive_root, 6_000).unwrap();
    assert_eq!(order.status, OrderStatus::Archived);
    assert_eq!(order.archived_at, Some(6_000));
    assert!(order.archive_path.is_some());
    assert!(order.dev_path.is_none());

    // Archive path must exist on disk
    let archive_path = order.archive_path.as_deref().unwrap();
    assert!(std::path::Path::new(archive_path).exists());

    // Original dev_path must no longer exist
    assert!(!std::path::Path::new(dev_path).exists());
}

#[test]
fn lead_lifecycle_promote_and_drop() {
    let tmp = TempDir::new().unwrap();
    let conn = make_conn(&tmp);

    // Create two leads
    let lead1 = create_order(
        &conn,
        &CreateOrderInput {
            title: "potential job A",
            slug: None,
            client_id: None,
            source_org: None,
            quoted_price: None,
            final_price: None,
            my_cut_ratio: 0.6,
            currency: "CNY",
            notes: None,
            as_lead: true,
        },
        1_000,
    )
    .unwrap();
    let lead2 = create_order(
        &conn,
        &CreateOrderInput {
            title: "potential job B",
            slug: None,
            client_id: None,
            source_org: None,
            quoted_price: None,
            final_price: None,
            my_cut_ratio: 0.6,
            currency: "CNY",
            notes: None,
            as_lead: true,
        },
        1_000,
    )
    .unwrap();

    // Promote lead1 → negotiating
    let promoted = transition(&conn, lead1.id, OrderStatus::Negotiating, 2_000).unwrap();
    assert_eq!(promoted.status, OrderStatus::Negotiating);

    // Drop lead2 → cancelled
    let dropped = transition(&conn, lead2.id, OrderStatus::Cancelled, 2_000).unwrap();
    assert_eq!(dropped.status, OrderStatus::Cancelled);
}
