//! `gig import` service: register an existing project directory into the DB.

use crate::config::Config;
use crate::context::is_within;
use crate::models::OrderStatus;
use crate::repo::{clients as client_repo, orders as order_repo};
use crate::services::lifecycle::add_tags;
use crate::{Error, Result};
use regex::Regex;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

// ─── Date prefix parsing ───────────────────────────────────────────────────────

static RE_FULL: OnceLock<Regex> = OnceLock::new();
static RE_SHORT: OnceLock<Regex> = OnceLock::new();

fn re_full() -> &'static Regex {
    RE_FULL.get_or_init(|| Regex::new(r"^(\d{4}-\d{2}-\d{2})[_-](.+)$").unwrap())
}

fn re_short() -> &'static Regex {
    RE_SHORT.get_or_init(|| Regex::new(r"^(\d{2}-\d{2})[_-](.+)$").unwrap())
}

/// Parse a date prefix from a directory name.
///
/// Returns `(unix_timestamp_or_None, rest_of_name)`.
pub fn parse_date_prefix(dir_name: &str) -> (Option<i64>, String) {
    // Try YYYY-MM-DD first
    if let Some(caps) = re_full().captures(dir_name) {
        let date_str = caps.get(1).unwrap().as_str();
        let rest = caps.get(2).unwrap().as_str().to_string();
        let ts = parse_ymd(date_str);
        return (ts, rest);
    }
    // Try MM-DD with current year
    if let Some(caps) = re_short().captures(dir_name) {
        let md_str = caps.get(1).unwrap().as_str();
        let rest = caps.get(2).unwrap().as_str().to_string();
        let year = current_year();
        let full = format!("{year}-{md_str}");
        let ts = parse_ymd(&full);
        return (ts, rest);
    }
    // No prefix — return full name as rest
    (None, dir_name.to_string())
}

fn parse_ymd(s: &str) -> Option<i64> {
    // s = "YYYY-MM-DD"
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i32 = parts[0].parse().ok()?;
    let m: u8 = parts[1].parse().ok()?;
    let d: u8 = parts[2].parse().ok()?;
    let date = time::Date::from_calendar_date(y, time::Month::try_from(m).ok()?, d).ok()?;
    let dt = time::PrimitiveDateTime::new(date, time::Time::MIDNIGHT);
    let offset = time::UtcOffset::UTC;
    Some(dt.assume_offset(offset).unix_timestamp())
}

fn current_year() -> i32 {
    time::OffsetDateTime::now_utc().year()
}

// ─── Slugify ──────────────────────────────────────────────────────────────────

/// Convert an arbitrary string into a URL-safe slug.
///
/// Lowercases, replaces non-`[a-z0-9]` with `-`, collapses runs, trims edges.
pub fn slugify(s: &str) -> String {
    let lower = s.to_lowercase();
    let replaced: String = lower
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    // Collapse consecutive hyphens and trim leading/trailing
    replaced
        .split('-')
        .filter(|seg| !seg.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// ─── Status inference ─────────────────────────────────────────────────────────

/// Infer the status of a project from its location on disk.
///
/// Priority (highest first):
/// 1. Path under `archive_root` → `archived`
/// 2. Path under `dev_root` with git commits → `in_progress`
/// 3. Path under `dev_root` without git history → `accepted`
/// 4. Anywhere else → `in_progress` (conservative default)
pub fn infer_status(path: &Path, dev_root: &Path, archive_root: &Path) -> OrderStatus {
    if is_within(path, archive_root) {
        return OrderStatus::Archived;
    }
    if is_within(path, dev_root) {
        if has_git_commits(path) {
            return OrderStatus::InProgress;
        } else {
            return OrderStatus::Accepted;
        }
    }
    OrderStatus::InProgress
}

fn has_git_commits(path: &Path) -> bool {
    let out = Command::new("git")
        .args(["-C", &path.to_string_lossy(), "log", "--oneline", "-1"])
        .output();
    match out {
        Ok(o) => o.status.success() && !o.stdout.is_empty(),
        Err(_) => false,
    }
}

// ─── Created-at inference ─────────────────────────────────────────────────────

/// Infer the created_at timestamp for a project.
///
/// Priority: date prefix > git first commit > dir metadata ctime.
pub fn infer_created_at(path: &Path, date_from_prefix: Option<i64>) -> i64 {
    if let Some(ts) = date_from_prefix {
        return ts;
    }
    if let Some(ts) = git_first_commit_time(path) {
        return ts;
    }
    dir_ctime(path)
}

fn git_first_commit_time(path: &Path) -> Option<i64> {
    let out = Command::new("git")
        .args([
            "-C",
            &path.to_string_lossy(),
            "log",
            "--reverse",
            "--format=%ct",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout.lines().next()?.trim().parse().ok()
}

fn dir_ctime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.created().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ─── Duplicate check ──────────────────────────────────────────────────────────

/// Check if a path is already registered in the DB (as dev_path or archive_path).
///
/// Returns `Some(order_id)` if already imported.
pub fn is_already_imported(conn: &Connection, canonical_path: &str) -> Result<Option<i64>> {
    let mut stmt =
        conn.prepare("SELECT id FROM orders WHERE dev_path = ?1 OR archive_path = ?1 LIMIT 1")?;
    let mut rows = stmt.query_map(rusqlite::params![canonical_path], |r| r.get::<_, i64>(0))?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

// ─── Unique slug generation ───────────────────────────────────────────────────

/// Return `base_slug` if unique, otherwise append `-2`, `-3`, etc.
pub fn ensure_unique_slug(conn: &Connection, base_slug: &str) -> Result<String> {
    let mut candidate = base_slug.to_string();
    let mut n = 2u32;
    loop {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM orders WHERE slug = ?1")?;
        let count: i64 = stmt.query_row(rusqlite::params![candidate], |r| r.get(0))?;
        if count == 0 {
            return Ok(candidate);
        }
        candidate = format!("{base_slug}-{n}");
        n += 1;
    }
}

// ─── Main import types ────────────────────────────────────────────────────────

/// Input for a single import operation.
pub struct ImportInput {
    pub path: PathBuf,
    /// Override for the slug (otherwise inferred from dir name).
    pub slug_override: Option<String>,
    /// Override for the title (otherwise derived from slug).
    pub title_override: Option<String>,
    /// Override for the status (otherwise inferred from location).
    pub status_override: Option<OrderStatus>,
    /// Optional quoted price (minor units).
    pub quoted_price: Option<i64>,
    /// Optional final price (minor units).
    pub final_price: Option<i64>,
    /// Optional client display_name (find_or_create).
    pub client_name: Option<String>,
    /// Optional source org name (display only).
    pub source_org: Option<String>,
    /// Optional source entity id (references sources table).
    pub source_id: Option<i64>,
    /// Optional notes.
    pub notes: Option<String>,
    /// Tags to attach (comma-separated or pre-split).
    pub tags: Vec<String>,
}

/// Result of a single import operation.
pub struct ImportResult {
    pub order_id: i64,
    pub slug: String,
    pub title: String,
    pub status: OrderStatus,
    pub path: PathBuf,
    /// If `Some`, the import was skipped (already imported as this order_id).
    pub skipped_as: Option<i64>,
}

// ─── Core import function ─────────────────────────────────────────────────────

/// Import a single project directory into the DB.
///
/// Returns an `ImportResult`; if the path is already imported the result has
/// `skipped_as = Some(existing_id)` and no DB write is performed.
pub fn import_project(
    conn: &Connection,
    input: &ImportInput,
    config: &Config,
    now: i64,
) -> Result<ImportResult> {
    // 1. Validate: must be a directory
    if !input.path.is_dir() {
        return Err(Error::Invalid(format!(
            "{} is not a directory",
            input.path.display()
        )));
    }

    // 2. Canonicalize
    let canonical = std::fs::canonicalize(&input.path)
        .map_err(|e| Error::PathUnavailable(input.path.clone(), e))?;
    let canonical_str = canonical
        .to_str()
        .ok_or_else(|| Error::Invalid("path is not valid UTF-8".into()))?
        .to_string();

    // 3. Duplicate check
    if let Some(existing_id) = is_already_imported(conn, &canonical_str)? {
        return Ok(ImportResult {
            order_id: existing_id,
            slug: String::new(),
            title: String::new(),
            status: OrderStatus::InProgress,
            path: canonical,
            skipped_as: Some(existing_id),
        });
    }

    // 4. Parse date prefix from directory name
    let dir_name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (date_ts, rest) = parse_date_prefix(&dir_name);

    // 5. Infer slug
    let base_slug = match &input.slug_override {
        Some(s) => s.clone(),
        None => {
            let raw = slugify(&rest);
            if raw.is_empty() {
                // fallback: use "import" — will get unique suffix
                "import".to_string()
            } else {
                raw
            }
        }
    };
    let slug = ensure_unique_slug(conn, &base_slug)?;

    // 6. Infer title
    let title = match &input.title_override {
        Some(t) => t.clone(),
        None => slug_to_title(&slug),
    };

    // 7. Infer status
    let dev_root = &config.general.dev_root;
    let archive_root = &config.general.archive_root;
    let status = match input.status_override {
        Some(s) => s,
        None => infer_status(&canonical, dev_root, archive_root),
    };

    // 8. Infer created_at
    let created_at = infer_created_at(&canonical, date_ts);

    // 9. Resolve client if provided
    let client_id = match &input.client_name {
        Some(name) if !name.is_empty() => Some(client_repo::find_or_create(conn, name, now)?.id),
        _ => None,
    };

    // 10. Build path fields
    let (dev_path, archive_path) = if status == OrderStatus::Archived {
        (None, Some(canonical_str.clone()))
    } else {
        (Some(canonical_str.clone()), None)
    };

    // 11. Compute timestamps
    let accepted_at = Some(created_at);
    let archived_at = if status == OrderStatus::Archived {
        Some(created_at)
    } else {
        None
    };

    // 12. Insert order
    let new_order = order_repo::NewOrder {
        slug: Some(&slug),
        external_id: None,
        title: &title,
        client_id,
        source_org: input.source_org.as_deref(),
        source_id: input.source_id,
        status,
        quoted_price: input.quoted_price,
        final_price: input.final_price,
        my_cut_ratio: config.general.default_cut_ratio,
        currency: &config.general.default_currency,
        notes: input.notes.as_deref(),
        created_at,
        accepted_at,
    };
    let order = order_repo::insert(conn, &new_order)?;

    // 13. Set path on inserted order
    if let Some(ref dp) = dev_path {
        conn.execute(
            "UPDATE orders SET dev_path = ?1 WHERE id = ?2",
            rusqlite::params![dp, order.id],
        )?;
    }
    if let Some(ref ap) = archive_path {
        conn.execute(
            "UPDATE orders SET archive_path = ?1 WHERE id = ?2",
            rusqlite::params![ap, order.id],
        )?;
    }
    // Set archived_at if needed
    if let Some(at) = archived_at {
        conn.execute(
            "UPDATE orders SET archived_at = ?1 WHERE id = ?2",
            rusqlite::params![at, order.id],
        )?;
    }

    // 14. Attach tags
    if !input.tags.is_empty() {
        let tag_refs: Vec<&str> = input.tags.iter().map(|s| s.as_str()).collect();
        add_tags(conn, order.id, &tag_refs)?;
    }

    Ok(ImportResult {
        order_id: order.id,
        slug,
        title,
        status,
        path: canonical,
        skipped_as: None,
    })
}

// ─── Relocate helper ──────────────────────────────────────────────────────────

/// Move an imported project directory to the standard location and update the DB.
///
/// Target is `<dev_root>/<slug>` for non-archived, `<archive_root>/<slug>` for archived.
pub fn relocate_project(conn: &Connection, order_id: i64, target_dir: &Path) -> Result<PathBuf> {
    let order = order_repo::find_by_id(conn, order_id)?;

    // Determine source path
    let src_str = order
        .dev_path
        .as_deref()
        .or(order.archive_path.as_deref())
        .ok_or_else(|| Error::Invalid(format!("order #{order_id} has no path stored")))?;
    let src = Path::new(src_str);

    if target_dir.exists() {
        return Err(Error::Invalid(format!(
            "target path already exists: {}",
            target_dir.display()
        )));
    }

    std::fs::rename(src, target_dir)
        .map_err(|e| Error::PathUnavailable(target_dir.to_path_buf(), e))?;

    let canonical = std::fs::canonicalize(target_dir)
        .map_err(|e| Error::PathUnavailable(target_dir.to_path_buf(), e))?;
    let canonical_str = canonical
        .to_str()
        .ok_or_else(|| Error::Invalid("path is not valid UTF-8".into()))?
        .to_string();

    if order.status == OrderStatus::Archived {
        conn.execute(
            "UPDATE orders SET archive_path = ?1, dev_path = NULL WHERE id = ?2",
            rusqlite::params![canonical_str, order_id],
        )?;
    } else {
        conn.execute(
            "UPDATE orders SET dev_path = ?1 WHERE id = ?2",
            rusqlite::params![canonical_str, order_id],
        )?;
    }

    Ok(canonical)
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn slug_to_title(slug: &str) -> String {
    slug.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, General};
    use crate::db::open_in_memory;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn default_config(dev_root: PathBuf, archive_root: PathBuf) -> Config {
        Config {
            general: General {
                dev_root,
                archive_root,
                default_cut_ratio: 0.6,
                default_currency: "CNY".into(),
            },
            ..Config::default()
        }
    }

    // ── parse_date_prefix ──

    #[test]
    fn parse_full_date_prefix() {
        let (ts, rest) = parse_date_prefix("2026-02-06_steam-game-review-scraper");
        assert!(ts.is_some());
        assert_eq!(rest, "steam-game-review-scraper");
    }

    #[test]
    fn parse_short_date_prefix() {
        let (ts, rest) = parse_date_prefix("04-08-human-action-live-recognization");
        assert!(ts.is_some());
        assert_eq!(rest, "human-action-live-recognization");
    }

    #[test]
    fn parse_no_date_prefix() {
        let (ts, rest) = parse_date_prefix("GingiCasc");
        assert!(ts.is_none());
        assert_eq!(rest, "GingiCasc");
    }

    // ── slugify ──

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Steam Game Review"), "steam-game-review");
    }

    #[test]
    fn slugify_weird_name() {
        assert_eq!(slugify("___weird---name___"), "weird-name");
    }

    #[test]
    fn slugify_empty() {
        assert_eq!(slugify(""), "");
    }

    // ── infer_status ──

    #[test]
    fn infer_status_under_archive_root() {
        let tmp = TempDir::new().unwrap();
        let dev = tmp.path().join("dev");
        let archive = tmp.path().join("archive");
        std::fs::create_dir_all(&archive).unwrap();
        let project = archive.join("my-project");
        std::fs::create_dir_all(&project).unwrap();
        let canonical_project = std::fs::canonicalize(&project).unwrap();
        let canonical_archive = std::fs::canonicalize(&archive).unwrap();
        let canonical_dev = dev.clone(); // doesn't need to exist for the test
        let status = infer_status(&canonical_project, &canonical_dev, &canonical_archive);
        assert_eq!(status, OrderStatus::Archived);
    }

    #[test]
    fn infer_status_elsewhere_defaults_in_progress() {
        let tmp = TempDir::new().unwrap();
        let dev = tmp.path().join("dev");
        let archive = tmp.path().join("archive");
        let elsewhere = tmp.path().join("other").join("project");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let status = infer_status(&elsewhere, &dev, &archive);
        assert_eq!(status, OrderStatus::InProgress);
    }

    // ── ensure_unique_slug ──

    #[test]
    fn ensure_unique_slug_first_call_returns_base() {
        let conn = open_in_memory().unwrap();
        let slug = ensure_unique_slug(&conn, "my-project").unwrap();
        assert_eq!(slug, "my-project");
    }

    #[test]
    fn ensure_unique_slug_second_call_appends_2() {
        let conn = open_in_memory().unwrap();
        // Insert an order with that slug
        let new = order_repo::NewOrder {
            slug: Some("my-project"),
            external_id: None,
            title: "My Project",
            client_id: None,
            source_org: None,
            source_id: None,
            status: OrderStatus::Accepted,
            quoted_price: None,
            final_price: None,
            my_cut_ratio: 0.6,
            currency: "CNY",
            notes: None,
            created_at: 0,
            accepted_at: None,
        };
        order_repo::insert(&conn, &new).unwrap();
        let slug = ensure_unique_slug(&conn, "my-project").unwrap();
        assert_eq!(slug, "my-project-2");
    }

    // ── import_project integration ──

    #[test]
    fn import_project_creates_order() {
        let tmp = TempDir::new().unwrap();
        let dev_root = tmp.path().join("dev");
        let archive_root = tmp.path().join("archive");
        std::fs::create_dir_all(&dev_root).unwrap();
        let project_dir = dev_root.join("2026-02-06_steam-game-review-scraper");
        std::fs::create_dir_all(&project_dir).unwrap();

        let conn = open_in_memory().unwrap();
        let config = default_config(dev_root, archive_root);
        let input = ImportInput {
            path: project_dir.clone(),
            slug_override: None,
            title_override: None,
            status_override: None,
            quoted_price: None,
            final_price: None,
            client_name: None,
            source_org: None,
            source_id: None,
            notes: None,
            tags: vec![],
        };
        let result = import_project(&conn, &input, &config, 1_000_000).unwrap();
        assert!(result.skipped_as.is_none());
        assert_eq!(result.slug, "steam-game-review-scraper");
        assert!(result.order_id > 0);
        // Verify order in DB
        let order = order_repo::find_by_id(&conn, result.order_id).unwrap();
        assert_eq!(order.slug.as_deref(), Some("steam-game-review-scraper"));
        assert!(order.dev_path.is_some() || order.archive_path.is_some());
    }

    #[test]
    fn import_project_duplicate_is_skipped() {
        let tmp = TempDir::new().unwrap();
        let dev_root = tmp.path().join("dev");
        let archive_root = tmp.path().join("archive");
        std::fs::create_dir_all(&dev_root).unwrap();
        let project_dir = dev_root.join("my-project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let conn = open_in_memory().unwrap();
        let config = default_config(dev_root, archive_root);
        let input = ImportInput {
            path: project_dir.clone(),
            slug_override: None,
            title_override: None,
            status_override: None,
            quoted_price: None,
            final_price: None,
            client_name: None,
            source_org: None,
            source_id: None,
            notes: None,
            tags: vec![],
        };

        let first = import_project(&conn, &input, &config, 1_000_000).unwrap();
        assert!(first.skipped_as.is_none());

        let second = import_project(&conn, &input, &config, 1_000_001).unwrap();
        assert_eq!(second.skipped_as, Some(first.order_id));
    }
}
