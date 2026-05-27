//! Integration tests for gig_core::context — the 9 correctness-contract tests
//! specified in §5.7.7 of the design spec.
//!
//! All tests use `resolve_context_for` with an explicit CWD to avoid
//! mutating process-global state (`std::env::set_current_dir`).

use gig_core::context::{canonical, is_within, resolve_context_for};
use gig_core::db::open_in_memory;
use gig_core::models::OrderStatus;
use gig_core::repo::orders::{insert, NewOrder};
use std::path::PathBuf;
use tempfile::TempDir;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn sample_order<'a>(title: &'a str, status: OrderStatus) -> NewOrder<'a> {
    NewOrder {
        slug: None,
        external_id: None,
        title,
        client_id: None,
        source_org: None,
        source_id: None,
        project_type: None,
        status,
        quoted_price: Some(100_000),
        final_price: Some(120_000),
        my_cut_ratio: 0.6,
        currency: "CNY",
        notes: None,
        created_at: 1_700_000_000,
        accepted_at: Some(1_700_000_000),
    }
}

fn insert_with_dev_path(
    conn: &rusqlite::Connection,
    title: &str,
    dev_path: &str,
) -> gig_core::models::Order {
    let order = insert(conn, &sample_order(title, OrderStatus::InProgress)).unwrap();
    gig_core::repo::orders::update_dev_path(conn, order.id, Some(dev_path)).unwrap();
    gig_core::repo::orders::find_by_id(conn, order.id).unwrap()
}

// ─── 1. Exact equality ──────────────────────────────────────────────────────

#[test]
fn exact_equality_cwd_equals_dev_path() {
    let tmp = TempDir::new().unwrap();
    let dev = tmp.path().join("project");
    std::fs::create_dir_all(&dev).unwrap();
    let dev_canon = canonical(&dev).unwrap();

    let conn = open_in_memory().unwrap();
    let order = insert_with_dev_path(&conn, "exact", dev_canon.to_str().unwrap());

    let result = resolve_context_for(&conn, &dev_canon).unwrap();
    assert_eq!(result, Some(order.id));
}

// ─── 2. True subdirectory ────────────────────────────────────────────────────

#[test]
fn true_subdirectory_several_levels_deep() {
    let tmp = TempDir::new().unwrap();
    let dev = tmp.path().join("project");
    let deep = dev.join("src").join("main").join("nested");
    std::fs::create_dir_all(&deep).unwrap();
    let dev_canon = canonical(&dev).unwrap();
    let deep_canon = canonical(&deep).unwrap();

    let conn = open_in_memory().unwrap();
    let order = insert_with_dev_path(&conn, "subdir", dev_canon.to_str().unwrap());

    let result = resolve_context_for(&conn, &deep_canon).unwrap();
    assert_eq!(result, Some(order.id));
}

// ─── 3. Similar prefix name (false-positive guard) ──────────────────────────

#[test]
fn similar_prefix_name_no_false_positive() {
    let tmp = TempDir::new().unwrap();
    let acme = tmp.path().join("acme");
    let acme_scraper = tmp.path().join("acme-scraper");
    std::fs::create_dir_all(&acme).unwrap();
    std::fs::create_dir_all(&acme_scraper).unwrap();
    let acme_canon = canonical(&acme).unwrap();
    let scraper_canon = canonical(&acme_scraper).unwrap();

    let conn = open_in_memory().unwrap();
    insert_with_dev_path(&conn, "acme", acme_canon.to_str().unwrap());

    // CWD is acme-scraper — must NOT match acme
    let result = resolve_context_for(&conn, &scraper_canon).unwrap();
    assert_eq!(result, None);
}

// ─── 4. Nested projects (deeper wins) ───────────────────────────────────────

#[test]
fn nested_projects_deeper_wins() {
    let tmp = TempDir::new().unwrap();
    let outer = tmp.path().join("workspace");
    let inner = outer.join("sub-project");
    std::fs::create_dir_all(&inner).unwrap();
    let outer_canon = canonical(&outer).unwrap();
    let inner_canon = canonical(&inner).unwrap();

    let conn = open_in_memory().unwrap();
    let _outer_order = insert_with_dev_path(&conn, "outer", outer_canon.to_str().unwrap());
    let inner_order = insert_with_dev_path(&conn, "inner", inner_canon.to_str().unwrap());

    // CWD inside inner → should match inner (deeper), not outer
    let result = resolve_context_for(&conn, &inner_canon).unwrap();
    assert_eq!(result, Some(inner_order.id));
}

// ─── 5. Outside any project ─────────────────────────────────────────────────

#[test]
fn outside_any_project_returns_none() {
    let tmp = TempDir::new().unwrap();
    let dev = tmp.path().join("project");
    let unrelated = tmp.path().join("elsewhere");
    std::fs::create_dir_all(&dev).unwrap();
    std::fs::create_dir_all(&unrelated).unwrap();
    let dev_canon = canonical(&dev).unwrap();
    let unrelated_canon = canonical(&unrelated).unwrap();

    let conn = open_in_memory().unwrap();
    insert_with_dev_path(&conn, "proj", dev_canon.to_str().unwrap());

    let result = resolve_context_for(&conn, &unrelated_canon).unwrap();
    assert_eq!(result, None);
}

// ─── 6. Symlink consistency ─────────────────────────────────────────────────

#[test]
fn symlink_consistency() {
    let tmp = TempDir::new().unwrap();
    let realdir = tmp.path().join("realdir");
    let link = tmp.path().join("link");
    std::fs::create_dir_all(&realdir).unwrap();
    std::os::unix::fs::symlink(&realdir, &link).unwrap();

    // Store the canonical (real) path in DB
    let real_canon = canonical(&realdir).unwrap();
    let conn = open_in_memory().unwrap();
    let order = insert_with_dev_path(&conn, "symlink-test", real_canon.to_str().unwrap());

    // CWD via symlink — canonicalize resolves to same real path → hit
    let link_canon = canonical(&link).unwrap();
    let result = resolve_context_for(&conn, &link_canon).unwrap();
    assert_eq!(result, Some(order.id));
}

// ─── 7. Deleted directory ───────────────────────────────────────────────────

#[test]
fn deleted_directory_graceful_none() {
    let tmp = TempDir::new().unwrap();
    let dev = tmp.path().join("project");
    let cwd_dir = tmp.path().join("my-cwd");
    std::fs::create_dir_all(&dev).unwrap();
    std::fs::create_dir_all(&cwd_dir).unwrap();
    let dev_canon = canonical(&dev).unwrap();
    let cwd_canon = canonical(&cwd_dir).unwrap();

    let conn = open_in_memory().unwrap();
    insert_with_dev_path(&conn, "will-delete", dev_canon.to_str().unwrap());

    // Delete the dev directory after storing its canonical path
    std::fs::remove_dir_all(&dev).unwrap();

    // resolve_context_for should gracefully return None (the stored path
    // is compared via is_within against CWD which is elsewhere — no match)
    let result = resolve_context_for(&conn, &cwd_canon).unwrap();
    assert_eq!(result, None);
}

// ─── 8. Trailing slash invariance ───────────────────────────────────────────

#[test]
fn trailing_slash_invariance() {
    let tmp = TempDir::new().unwrap();
    let dev = tmp.path().join("project");
    std::fs::create_dir_all(&dev).unwrap();
    let dev_canon = canonical(&dev).unwrap();
    let dev_str = dev_canon.to_str().unwrap();

    // Store path WITH trailing slash
    let with_slash = format!("{}/", dev_str);
    let conn = open_in_memory().unwrap();
    let order = insert(
        &conn,
        &sample_order("trailing-slash", OrderStatus::InProgress),
    )
    .unwrap();
    gig_core::repo::orders::update_dev_path(&conn, order.id, Some(&with_slash)).unwrap();

    // PathBuf::from("/foo/bar/") normalises to "/foo/bar" on component iteration,
    // so is_within still works.
    let result = resolve_context_for(&conn, &dev_canon).unwrap();
    assert_eq!(result, Some(order.id));

    // Also test: CWD with trailing slash (PathBuf normalises it)
    let cwd_with_slash = PathBuf::from(&with_slash);
    // is_within should still detect a match
    assert!(is_within(&cwd_with_slash, &dev_canon));
}

// ─── 9. Empty candidate set ─────────────────────────────────────────────────

#[test]
fn empty_candidate_set_returns_none() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path().join("somewhere");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd_canon = canonical(&cwd).unwrap();

    let conn = open_in_memory().unwrap();
    // No orders at all
    let result = resolve_context_for(&conn, &cwd_canon).unwrap();
    assert_eq!(result, None);
}
