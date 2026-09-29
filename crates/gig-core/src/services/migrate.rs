//! v1 -> v2 migration. Reads the old database read-only, maps every row per
//! docs/v2/SPEC.md section 6, and writes a fresh v2 database.

use crate::clock::{self, LegacyStamp};
use crate::models::{Channel, DraftStatus, OrderStatus, PackageKind, PackageStatus, ProjectType};
use crate::repo::{self, artifacts, drafts, orders as repo_orders, packages};
use crate::{db, Error, Result};
use rusqlite::types::Value;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Options {
    pub from: PathBuf,
    pub to: PathBuf,
    pub dry_run: bool,
    /// (old prefix, new prefix) rewrites applied to dev_path / archive_path.
    pub fix_paths: Vec<(String, String)>,
    pub warranty_days: i64,
}

/// (old_minor, new_minor, reason, created_at)
pub type PriceEntry = (Option<i64>, Option<i64>, Option<String>, String);

#[derive(Debug, Serialize)]
pub struct Report {
    pub from: String,
    pub to: String,
    pub dry_run: bool,
    pub counts: BTreeMap<String, usize>,
    pub orders: Vec<MappedOrder>,
    pub drafts: Vec<MappedDraft>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MappedOrder {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub v1_status: String,
    pub status: OrderStatus,
    pub project_type: ProjectType,
    pub currency: String,
    pub price_minor: Option<i64>,
    pub cut_ratio: f64,
    pub platform: Option<String>,
    pub external_id: Option<String>,
    pub dev_path: Option<String>,
    pub archive_path: Option<String>,
    pub notes: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub delivered_at: Option<String>,
    pub paid_at: Option<String>,
    pub warranty_until: Option<String>,
    pub archived_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub cancel_reason: Option<String>,
    pub packages: Vec<MappedPackage>,
    pub artifacts: usize,
    pub price_history: Vec<PriceEntry>,
    pub requirement_changes: Vec<(String, i64, String)>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MappedPackage {
    pub package_id: String,
    pub status: PackageStatus,
    pub zip_path: String,
    pub checked_at: Option<String>,
    pub sent_at: Option<String>,
    pub remote_url: Option<String>,
    pub file_exists: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct MappedDraft {
    pub slug: String,
    pub title: Option<String>,
    pub project_type: Option<ProjectType>,
    pub notes_dir: String,
    pub drop_reason: Option<String>,
    pub notes_snapshot: String,
    pub created_at: String,
}

struct Row(Vec<(String, Value)>);

impl Row {
    fn get(&self, col: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == col).map(|(_, v)| v)
    }
    fn text(&self, col: &str) -> Option<String> {
        match self.get(col)? {
            Value::Text(t) => Some(t.clone()).filter(|t| !t.is_empty()),
            Value::Integer(i) => Some(i.to_string()),
            Value::Real(r) => Some(r.to_string()),
            _ => None,
        }
    }
    fn int(&self, col: &str) -> Option<i64> {
        match self.get(col)? {
            Value::Integer(i) => Some(*i),
            Value::Text(t) => t.trim().parse().ok(),
            Value::Real(r) => Some(*r as i64),
            _ => None,
        }
    }
    fn float(&self, col: &str) -> Option<f64> {
        match self.get(col)? {
            Value::Real(r) => Some(*r),
            Value::Integer(i) => Some(*i as f64),
            Value::Text(t) => t.trim().parse().ok(),
            _ => None,
        }
    }
    fn stamp(&self, col: &str) -> Option<LegacyStamp> {
        match self.get(col)? {
            Value::Integer(i) => Some(LegacyStamp::Int(*i)),
            Value::Text(t) if !t.trim().is_empty() => Some(LegacyStamp::Text(t.clone())),
            _ => None,
        }
    }
}

fn read_table(conn: &Connection, table: &str) -> Result<Vec<Row>> {
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt.query_map([], |r| {
        let mut out = Vec::with_capacity(names.len());
        for (i, n) in names.iter().enumerate() {
            out.push((n.clone(), r.get::<_, Value>(i)?));
        }
        Ok(Row(out))
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn map_status(v1: &str) -> Result<(OrderStatus, Option<&'static str>)> {
    Ok(match v1 {
        "accepted" | "plan_ready" | "plan_approved" => (OrderStatus::Queued, None),
        "in_progress" | "ready_to_deliver" | "revision" => (OrderStatus::InProgress, None),
        "delivered" => (OrderStatus::Delivered, None),
        "paid" => (OrderStatus::Paid, None),
        "archived" => (OrderStatus::Archived, None),
        "cancelled" => (OrderStatus::Cancelled, None),
        "lead" | "negotiating" => (OrderStatus::Cancelled, Some("legacy lead")),
        other => return Err(Error::InvalidInput(format!("unknown v1 status {other:?}"))),
    })
}

fn map_type(v1: Option<String>) -> ProjectType {
    match v1.as_deref() {
        Some("cv_ml") => ProjectType::CvMl,
        Some("data_processing") => ProjectType::DataProcessing,
        Some("research_writing") => ProjectType::ResearchWriting,
        Some("tool") => ProjectType::Tool,
        _ => ProjectType::Custom,
    }
}

fn stamp_or_warn(row: &Row, col: &str, label: &str, warnings: &mut Vec<String>) -> Option<String> {
    let s = row.stamp(col)?;
    match clock::legacy_to_rfc3339(&s) {
        Ok(t) => Some(t),
        Err(e) => {
            warnings.push(format!("{label}: {col} unparseable ({e})"));
            None
        }
    }
}

fn rewrite(
    path: Option<String>,
    fixes: &[(String, String)],
    warnings: &mut Vec<String>,
    label: &str,
) -> Option<String> {
    let p = path?;
    for (old, new) in fixes {
        if p == *old || p.starts_with(&format!("{old}/")) {
            let rewritten = format!("{new}{}", &p[old.len()..]);
            warnings.push(format!("{label}: path {p} -> {rewritten}"));
            return Some(rewritten);
        }
    }
    Some(p)
}

fn absolute(path: &str, root: Option<&str>) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        path.to_string()
    } else {
        root.map(|r| Path::new(r).join(p).to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string())
    }
}

pub fn plan(opts: &Options) -> Result<Report> {
    let conn = db::open_legacy_readonly(&opts.from)?;
    let mut warnings = Vec::new();
    let mut counts = BTreeMap::new();

    let v1_orders = read_table(&conn, "orders")?;
    let v1_price = read_table(&conn, "price_history")?;
    let v1_changes = read_table(&conn, "requirement_changes")?;
    let v1_packages = read_table(&conn, "delivery_packages")?;
    let v1_artifacts = read_table(&conn, "delivery_artifacts")?;
    let v1_quotes = if table_exists(&conn, "quote_drafts")? {
        read_table(&conn, "quote_drafts")?
    } else {
        vec![]
    };
    let v1_tags = if table_exists(&conn, "tags")? {
        read_table(&conn, "tags")?
    } else {
        vec![]
    };
    let v1_order_tags = if table_exists(&conn, "order_tags")? {
        read_table(&conn, "order_tags")?
    } else {
        vec![]
    };
    counts.insert("v1.orders".into(), v1_orders.len());
    counts.insert("v1.price_history".into(), v1_price.len());
    counts.insert("v1.requirement_changes".into(), v1_changes.len());
    counts.insert("v1.delivery_packages".into(), v1_packages.len());
    counts.insert("v1.delivery_artifacts".into(), v1_artifacts.len());
    counts.insert("v1.quote_drafts".into(), v1_quotes.len());

    let tag_names: BTreeMap<i64, String> = v1_tags
        .iter()
        .filter_map(|t| Some((t.int("id")?, t.text("name")?)))
        .collect();

    let mut orders = Vec::new();
    for row in &v1_orders {
        let id = row
            .int("id")
            .ok_or_else(|| Error::InvalidInput("v1 order without id".into()))?;
        let label = format!("order #{id}");
        let slug = row.text("slug").ok_or_else(|| {
            Error::InvalidInput(format!(
                "{label} has no slug; v1 orders must have slugs before migrating"
            ))
        })?;
        let v1_status = row.text("status").unwrap_or_default();
        let (status, cancel_reason) = map_status(&v1_status)?;
        let quoted = row.int("quoted_price");
        let final_ = row.int("final_price");
        let price_minor = final_.or(quoted);
        if price_minor == Some(0) {
            warnings.push(format!("{label}: zero price"));
        }
        let mut price_history: Vec<PriceEntry> = Vec::new();
        let created_at =
            stamp_or_warn(row, "created_at", &label, &mut warnings).unwrap_or_else(clock::now);
        if let (Some(q), Some(f)) = (quoted, final_) {
            if q != f {
                price_history.push((
                    Some(q),
                    Some(f),
                    Some("v1 quoted -> final".into()),
                    created_at.clone(),
                ));
            }
        }
        for p in v1_price.iter().filter(|p| p.int("order_id") == Some(id)) {
            let at = stamp_or_warn(
                p,
                "created_at",
                &format!("{label} price_history"),
                &mut warnings,
            )
            .unwrap_or_else(|| created_at.clone());
            price_history.push((p.int("old_price"), p.int("new_price"), p.text("reason"), at));
        }
        let requirement_changes: Vec<(String, i64, String)> = v1_changes
            .iter()
            .filter(|c| c.int("order_id") == Some(id))
            .map(|c| {
                let at = stamp_or_warn(
                    c,
                    "created_at",
                    &format!("{label} requirement_change"),
                    &mut warnings,
                )
                .unwrap_or_else(|| created_at.clone());
                (
                    c.text("description").unwrap_or_default(),
                    c.int("price_delta").unwrap_or(0),
                    at,
                )
            })
            .collect();

        let mut notes = row.text("notes").unwrap_or_default();
        if !notes.is_empty() && !notes.ends_with('\n') {
            notes.push('\n');
        }
        let tags: Vec<String> = v1_order_tags
            .iter()
            .filter(|t| t.int("order_id") == Some(id))
            .filter_map(|t| tag_names.get(&t.int("tag_id")?).cloned())
            .collect();
        if !tags.is_empty() {
            notes.push_str(&format!("[{created_at}] tags: {}\n", tags.join(", ")));
        }
        for q in v1_quotes
            .iter()
            .filter(|q| q.int("promoted_order_id") == Some(id))
        {
            let range = [
                q.int("quote_min"),
                q.int("quote_recommended"),
                q.int("quote_max"),
            ]
            .iter()
            .map(|v| {
                v.map(crate::money::format_minor)
                    .unwrap_or_else(|| "-".into())
            })
            .collect::<Vec<_>>()
            .join("/");
            notes.push_str(&format!(
                "[{created_at}] quote draft {}: {}; min/rec/max {range}\n",
                q.text("slug").unwrap_or_default(),
                q.text("summary").unwrap_or_default()
            ));
        }

        let dev_path = rewrite(row.text("dev_path"), &opts.fix_paths, &mut warnings, &label);
        let archive_path = rewrite(
            row.text("archive_path"),
            &opts.fix_paths,
            &mut warnings,
            &label,
        );
        let root = if status == OrderStatus::Archived {
            archive_path.clone()
        } else {
            dev_path.clone()
        };

        let paid_at = stamp_or_warn(row, "paid_at", &label, &mut warnings);
        let paid_date = paid_at.as_deref().map(clock::date_of).transpose()?;
        let warranty_until = paid_date
            .as_deref()
            .map(|d| clock::date_plus_days(d, opts.warranty_days))
            .transpose()?;
        let accepted_at = stamp_or_warn(row, "accepted_at", &label, &mut warnings);
        let started_at = if matches!(status, OrderStatus::Queued | OrderStatus::Cancelled) {
            None
        } else {
            accepted_at.clone().or_else(|| Some(created_at.clone()))
        };

        // Packages: merge v1 rows that name the same zip.
        let mut merged: BTreeMap<String, MappedPackage> = BTreeMap::new();
        let mut unnamed = 0;
        for p in v1_packages.iter().filter(|p| p.int("order_id") == Some(id)) {
            let plabel = format!("{label} package #{}", p.int("id").unwrap_or(0));
            let zip_rel = p.text("package_path");
            let zip_abs = zip_rel.as_deref().map(|z| absolute(z, root.as_deref()));
            let key = match &zip_abs {
                Some(z) => z.clone(),
                None => {
                    unnamed += 1;
                    format!(
                        "legacy-{}-{unnamed}",
                        p.text("delivery_date").unwrap_or_default()
                    )
                }
            };
            let package_id = match &zip_abs {
                Some(z) => Path::new(z)
                    .file_name()
                    .map(|f| f.to_string_lossy().trim_end_matches(".zip").to_string())
                    .unwrap_or_else(|| key.clone()),
                None => key.clone(),
            };
            let at = stamp_or_warn(p, "updated_at", &plabel, &mut warnings)
                .or_else(|| stamp_or_warn(p, "created_at", &plabel, &mut warnings));
            let v1s = p.text("status").unwrap_or_default();
            let entry = merged.entry(key.clone()).or_insert_with(|| MappedPackage {
                package_id: package_id.clone(),
                status: PackageStatus::Legacy,
                zip_path: zip_abs.clone().unwrap_or_else(|| key.clone()),
                checked_at: None,
                sent_at: None,
                remote_url: None,
                file_exists: zip_abs.as_deref().is_some_and(|z| Path::new(z).is_file()),
            });
            match v1s.as_str() {
                "sent" => {
                    entry.status = PackageStatus::Sent;
                    if at.as_deref() > entry.sent_at.as_deref() {
                        entry.sent_at = at.clone();
                    }
                }
                "validated" => {
                    if entry.status != PackageStatus::Sent {
                        entry.status = PackageStatus::Checked;
                    }
                    if at.as_deref() > entry.checked_at.as_deref() {
                        entry.checked_at = at.clone();
                    }
                }
                _ => {}
            }
            if let Some(a) = v1_artifacts.iter().find(|a| {
                a.int("order_id") == Some(id)
                    && a.text("local_path")
                        .as_deref()
                        .map(|l| absolute(l, root.as_deref()))
                        == zip_abs
            }) {
                if entry.remote_url.is_none() {
                    entry.remote_url = a.text("remote_url");
                }
            }
        }
        // Distinct zips that share a basename inside one order get a suffix.
        let mut seen_ids: BTreeMap<String, usize> = BTreeMap::new();
        let mut pkgs: Vec<MappedPackage> = merged.into_values().collect();
        for p in &mut pkgs {
            let n = seen_ids.entry(p.package_id.clone()).or_insert(0);
            *n += 1;
            if *n > 1 {
                p.package_id = format!("{}-{}", p.package_id, n);
            }
            if !p.file_exists {
                warnings.push(format!(
                    "{label}: package {} zip missing at {}",
                    p.package_id, p.zip_path
                ));
            }
        }
        let artifact_count = v1_artifacts
            .iter()
            .filter(|a| a.int("order_id") == Some(id))
            .count();

        orders.push(MappedOrder {
            id,
            slug,
            title: row.text("title").unwrap_or_default(),
            v1_status,
            status,
            project_type: map_type(row.text("project_type")),
            currency: row.text("currency").unwrap_or_else(|| "CNY".into()),
            price_minor,
            cut_ratio: row.float("my_cut_ratio").unwrap_or(0.6),
            platform: row.text("source_org"),
            external_id: row.text("external_id"),
            dev_path,
            archive_path,
            notes,
            created_at,
            started_at,
            delivered_at: stamp_or_warn(row, "delivered_at", &label, &mut warnings),
            paid_at: paid_date,
            warranty_until,
            archived_at: stamp_or_warn(row, "archived_at", &label, &mut warnings),
            cancelled_at: if status == OrderStatus::Cancelled {
                Some(clock::now())
            } else {
                None
            },
            cancel_reason: cancel_reason.map(str::to_string),
            packages: pkgs,
            artifacts: artifact_count,
            price_history,
            requirement_changes,
        });
    }

    let mut mapped_drafts = Vec::new();
    for q in v1_quotes
        .iter()
        .filter(|q| q.int("promoted_order_id").is_none())
    {
        let slug = q.text("slug").unwrap_or_default();
        let label = format!("quote draft {slug}");
        let xdg = q.text("xdg_path").unwrap_or_default();
        let mut snapshot = q.text("summary").unwrap_or_default();
        let mut any_file = false;
        for f in ["JOB.md", "QUOTE.md"] {
            let p = Path::new(&xdg).join(f);
            if let Ok(t) = std::fs::read_to_string(&p) {
                any_file = true;
                snapshot.push_str(&format!("\n\n---- {f} ----\n{t}"));
            }
        }
        if !any_file {
            warnings.push(format!("{label}: no files at {xdg}; summary only"));
        }
        mapped_drafts.push(MappedDraft {
            slug,
            title: q.text("title"),
            project_type: q.text("project_type").map(Some).map(map_type),
            notes_dir: xdg,
            drop_reason: q
                .text("drop_reason")
                .or_else(|| Some("legacy quote draft".into())),
            notes_snapshot: snapshot,
            created_at: stamp_or_warn(q, "created_at", &label, &mut warnings)
                .unwrap_or_else(clock::now),
        });
    }

    counts.insert("v2.orders".into(), orders.len());
    counts.insert(
        "v2.packages".into(),
        orders.iter().map(|o| o.packages.len()).sum(),
    );
    counts.insert(
        "v2.artifacts".into(),
        orders.iter().map(|o| o.artifacts).sum(),
    );
    counts.insert(
        "v2.price_history".into(),
        orders.iter().map(|o| o.price_history.len()).sum(),
    );
    counts.insert(
        "v2.requirement_changes".into(),
        orders.iter().map(|o| o.requirement_changes.len()).sum(),
    );
    counts.insert("v2.drafts".into(), mapped_drafts.len());

    Ok(Report {
        from: opts.from.to_string_lossy().into_owned(),
        to: opts.to.to_string_lossy().into_owned(),
        dry_run: opts.dry_run,
        counts,
        orders,
        drafts: mapped_drafts,
        warnings,
    })
}

pub fn run(opts: &Options) -> Result<Report> {
    let report = plan(opts)?;
    if opts.dry_run {
        return Ok(report);
    }
    if opts.to.exists() {
        return Err(Error::InvalidInput(format!(
            "{} already exists; refusing to overwrite",
            opts.to.display()
        )));
    }
    let src = db::open_legacy_readonly(&opts.from)?;
    let v1_artifacts = read_table(&src, "delivery_artifacts")?;
    let conn = db::open(&opts.to)?;
    let tx = conn.unchecked_transaction()?;
    let now = clock::now();
    for o in &report.orders {
        repo_orders::insert(
            &tx,
            &repo_orders::NewOrder {
                id: Some(o.id),
                slug: &o.slug,
                title: &o.title,
                material_path: None,
                platform: o.platform.as_deref(),
                external_id: o.external_id.as_deref(),
                project_type: o.project_type,
                status: o.status,
                currency: &o.currency,
                price_minor: o.price_minor,
                cut_ratio: o.cut_ratio,
                dev_path: o.dev_path.as_deref(),
                archive_path: o.archive_path.as_deref(),
                client_words: None,
                notes: &o.notes,
                created_at: &o.created_at,
                started_at: o.started_at.as_deref(),
                delivered_at: o.delivered_at.as_deref(),
                paid_at: o.paid_at.as_deref(),
                warranty_until: o.warranty_until.as_deref(),
                archived_at: o.archived_at.as_deref(),
                cancelled_at: o.cancelled_at.as_deref(),
                cancel_reason: o.cancel_reason.as_deref(),
                legacy_id: Some(o.id),
            },
        )?;
        for (old, new, reason, at) in &o.price_history {
            repo::insert_price_history(&tx, o.id, *old, *new, reason.as_deref(), at)?;
        }
        for (desc, delta, at) in &o.requirement_changes {
            repo::insert_requirement_change(&tx, o.id, desc, *delta, at)?;
        }
        for p in &o.packages {
            let dir = p.zip_path.trim_end_matches(".zip").to_string();
            packages::upsert(
                &tx,
                &packages::NewPackage {
                    order_id: o.id,
                    package_id: &p.package_id,
                    kind: PackageKind::Full,
                    dir: &dir,
                    manifest_path: "",
                    zip_path: &p.zip_path,
                    zip_sha256: None,
                    file_count: None,
                    status: p.status,
                    checked_at: p.checked_at.as_deref(),
                    sent_at: p.sent_at.as_deref(),
                    channel: p.sent_at.as_ref().map(|_| Channel::Oss),
                    uploader: None,
                    remote_url: p.remote_url.as_deref(),
                    short_url: None,
                    expires_at: None,
                    created_at: p
                        .checked_at
                        .as_deref()
                        .or(p.sent_at.as_deref())
                        .unwrap_or(&now),
                },
            )?;
        }
        for a in v1_artifacts
            .iter()
            .filter(|a| a.int("order_id") == Some(o.id))
        {
            let uploaded_at = a
                .stamp("uploaded_at")
                .and_then(|s| clock::legacy_to_rfc3339(&s).ok())
                .unwrap_or_else(|| o.created_at.clone());
            let expires_at = a
                .stamp("expires_at")
                .and_then(|s| clock::legacy_to_rfc3339(&s).ok());
            artifacts::insert(
                &tx,
                &artifacts::NewArtifact {
                    order_id: o.id,
                    local_path: a.text("local_path").as_deref(),
                    uploader: a.text("uploader_name").as_deref(),
                    remote_url: a.text("remote_url").as_deref(),
                    short_url: None,
                    expires_at: expires_at.as_deref(),
                    uploaded_at: &uploaded_at,
                },
            )?;
        }
    }
    for d in &report.drafts {
        drafts::insert(
            &tx,
            &drafts::NewDraft {
                slug: &d.slug,
                title: d.title.as_deref(),
                material_path: None,
                project_type: d.project_type,
                notes_dir: &d.notes_dir,
                status: DraftStatus::Dropped,
                drop_reason: d.drop_reason.as_deref(),
                notes_snapshot: Some(&d.notes_snapshot),
                promoted_order_id: None,
                created_at: &d.created_at,
                closed_at: Some(&now),
            },
        )?;
    }
    tx.commit()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A v1 database with the real v1 schema and synthetic rows covering every mapping.
    fn v1_fixture(dir: &Path) -> PathBuf {
        let p = dir.join("gig.db");
        let c = Connection::open(&p).unwrap();
        let schema = include_str!("../../tests/fixtures/v1/schema.sql");
        c.execute_batch(schema).unwrap();
        c.execute_batch(&format!(r#"
INSERT INTO orders (id, slug, title, source_org, status, quoted_price, final_price, my_cut_ratio, currency, dev_path, notes, created_at, accepted_at, delivered_at, paid_at, archived_at, project_type, external_id)
VALUES (1, 'old-paid', 'Old paid', 'platform-x', 'archived', 250000, 250000, 0.6, 'CNY', '{d}/old-paid', 'hello', 1775606400, 1775606400, NULL, 1776256502, 1776256600, 'cv_ml', 'EXT-1');
INSERT INTO orders (id, slug, title, status, quoted_price, final_price, my_cut_ratio, currency, dev_path, created_at, accepted_at, project_type)
VALUES (2, 'renamed', 'Renamed dir', 'accepted', NULL, NULL, 0.6, 'CNY', '{d}/old-name', 1780943044, 1780943044, 'research_writing');
INSERT INTO orders (id, slug, title, status, quoted_price, final_price, my_cut_ratio, currency, dev_path, created_at, accepted_at, delivered_at, project_type)
VALUES (3, 'delivered', 'Delivered unpaid', 'delivered', 80000, 80000, 0.6, 'CNY', '{d}/delivered', 1787326471, 1787326471, 1787400000, 'cv_ml');
INSERT INTO orders (id, slug, title, status, quoted_price, final_price, my_cut_ratio, currency, created_at)
VALUES (4, 'zero', 'Zero price', 'archived', 0, NULL, 0.6, 'CNY', 1780456708);
INSERT INTO orders (id, slug, title, status, quoted_price, final_price, my_cut_ratio, currency, created_at)
VALUES (5, 'lead', 'A lead', 'lead', NULL, NULL, 0.6, 'CNY', 1780456708);
INSERT INTO orders (id, slug, title, status, quoted_price, final_price, my_cut_ratio, currency, created_at)
VALUES (6, 'quoted-differs', 'Quoted differs', 'paid', 100000, 120000, 0.6, 'CNY', 1780456708);
INSERT INTO price_history (order_id, old_price, new_price, reason, created_at) VALUES (1, 200000, 250000, 'scope', 1775700000);
INSERT INTO requirement_changes (order_id, description, price_delta, created_at) VALUES (1, 'add export', 50000, 1775700000);
INSERT INTO delivery_packages (order_id, delivery_date, delivery_dir, client_dir, manifest_path, package_path, status, created_at, updated_at)
VALUES (3, '2026-08-26', '.gig/delivery/2026-08-26', '.gig/delivery/2026-08-26/client', '.gig/delivery/2026-08-26/manifest.toml', '.gig/delivery/2026-08-26/export/delivered-2026-08-26.zip', 'validated', '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z');
INSERT INTO delivery_packages (order_id, delivery_date, delivery_dir, client_dir, manifest_path, package_path, status, created_at, updated_at)
VALUES (3, '2026-08-26', '{d}/delivered/.gig/delivery/2026-08-26', '{d}/delivered/.gig/delivery/2026-08-26/client', '{d}/delivered/.gig/delivery/2026-08-26/manifest.toml', '{d}/delivered/.gig/delivery/2026-08-26/export/delivered-2026-08-26.zip', 'sent', '2026-08-26T11:00:00Z', '2026-08-26T11:00:00Z');
INSERT INTO delivery_artifacts (order_id, local_path, uploader_name, remote_url, expires_at, uploaded_at)
VALUES (3, '{d}/delivered/.gig/delivery/2026-08-26/export/delivered-2026-08-26.zip', 's3:bj', 'https://go.example/abc', 1788000000, 1787400000);
INSERT INTO tags (id, name) VALUES (1, 'urgent');
INSERT INTO order_tags (order_id, tag_id) VALUES (1, 1);
INSERT INTO quote_drafts (slug, title, project_type, status, summary, quote_min, quote_recommended, quote_max, xdg_path, promoted_order_id, created_at, updated_at)
VALUES ('promoted-q', 'PQ', 'cv_ml', 'accepted', 'promoted summary', 60000, 80000, 100000, '{d}/quotes/promoted-q', 3, '2026-08-20T00:00:00Z', '2026-08-20T00:00:00Z');
INSERT INTO quote_drafts (slug, title, project_type, status, summary, xdg_path, drop_reason, created_at, updated_at)
VALUES ('dropped-q', 'DQ', 'custom', 'dropped', 'dropped summary', '{d}/quotes/dropped-q', 'no budget', '2026-08-21T00:00:00Z', '2026-08-21T00:00:00Z');
INSERT INTO order_workflow (order_id, created_at, updated_at) VALUES (3, 'x', 'x');
"#, d = dir.display())).unwrap();
        p
    }

    #[test]
    fn maps_every_row() {
        let dir = tempfile::tempdir().unwrap();
        let from = v1_fixture(dir.path());
        std::fs::create_dir_all(dir.path().join("delivered/.gig/delivery/2026-08-26/export"))
            .unwrap();
        std::fs::write(
            dir.path()
                .join("delivered/.gig/delivery/2026-08-26/export/delivered-2026-08-26.zip"),
            "z",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("quotes/dropped-q")).unwrap();
        std::fs::write(dir.path().join("quotes/dropped-q/JOB.md"), "job text").unwrap();
        let to = dir.path().join("gig-v2.db");
        let opts = Options {
            from: from.clone(),
            to: to.clone(),
            dry_run: true,
            fix_paths: vec![(
                dir.path().join("old-name").to_string_lossy().into_owned(),
                dir.path().join("renamed").to_string_lossy().into_owned(),
            )],
            warranty_days: 15,
        };
        let r = run(&opts).unwrap();
        assert!(r.dry_run);
        assert!(!to.exists());
        assert_eq!(r.counts["v2.orders"], 6);
        let by_slug = |s: &str| r.orders.iter().find(|o| o.slug == s).unwrap().clone();

        let paid = by_slug("old-paid");
        assert_eq!(paid.status, OrderStatus::Archived);
        assert_eq!(paid.platform.as_deref(), Some("platform-x"));
        assert_eq!(paid.external_id.as_deref(), Some("EXT-1"));
        assert_eq!(paid.paid_at.as_deref(), Some("2026-04-15"));
        assert_eq!(paid.warranty_until.as_deref(), Some("2026-04-30"));
        assert_eq!(paid.created_at, "2026-04-08T00:00:00Z");
        assert!(paid.notes.contains("hello") && paid.notes.contains("tags: urgent"));
        assert_eq!(paid.price_history.len(), 1);
        assert_eq!(paid.requirement_changes.len(), 1);

        let renamed = by_slug("renamed");
        assert_eq!(renamed.status, OrderStatus::Queued);
        assert!(renamed.dev_path.as_deref().unwrap().ends_with("/renamed"));
        assert!(renamed.started_at.is_none());

        let delivered = by_slug("delivered");
        assert_eq!(
            delivered.packages.len(),
            1,
            "same zip merges: {:?}",
            delivered.packages
        );
        let p = &delivered.packages[0];
        assert_eq!(p.package_id, "delivered-2026-08-26");
        assert_eq!(p.status, PackageStatus::Sent);
        assert_eq!(p.checked_at.as_deref(), Some("2026-08-26T10:00:00Z"));
        assert_eq!(p.sent_at.as_deref(), Some("2026-08-26T11:00:00Z"));
        assert_eq!(p.remote_url.as_deref(), Some("https://go.example/abc"));
        assert!(p.file_exists);
        assert!(delivered.notes.contains("quote draft promoted-q"));
        assert!(delivered.notes.contains("600.00/800.00/1000.00"));

        assert_eq!(by_slug("zero").price_minor, Some(0));
        assert!(r.warnings.iter().any(|w| w.contains("zero price")));
        let lead = by_slug("lead");
        assert_eq!(lead.status, OrderStatus::Cancelled);
        assert_eq!(lead.cancel_reason.as_deref(), Some("legacy lead"));
        let qd = by_slug("quoted-differs");
        assert_eq!(qd.price_minor, Some(120000));
        assert_eq!(qd.price_history.len(), 1);

        assert_eq!(r.drafts.len(), 1);
        assert_eq!(r.drafts[0].slug, "dropped-q");
        assert!(r.drafts[0].notes_snapshot.contains("job text"));
        assert_eq!(r.drafts[0].drop_reason.as_deref(), Some("no budget"));

        // apply
        let r2 = run(&Options {
            dry_run: false,
            ..opts
        })
        .unwrap();
        assert!(!r2.dry_run);
        let conn = db::open(&to).unwrap();
        let all = repo_orders::list(&conn, true).unwrap();
        assert_eq!(all.len(), 6);
        let o3 = repo_orders::find_by_id(&conn, 3).unwrap();
        assert_eq!(o3.legacy_id, Some(3));
        assert_eq!(o3.status, OrderStatus::Delivered);
        assert_eq!(packages::list_for_order(&conn, 3).unwrap().len(), 1);
        assert_eq!(artifacts::list_for_order(&conn, 3).unwrap().len(), 1);
        assert_eq!(drafts::list(&conn, true).unwrap().len(), 1);
        // refuses to overwrite
        let e = run(&Options {
            from,
            to,
            dry_run: false,
            fix_paths: vec![],
            warranty_days: 15,
        })
        .unwrap_err();
        assert_eq!(e.code(), "invalid_input");
    }
}
