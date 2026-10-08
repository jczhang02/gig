//! Consistency checks over paths, packages, project files, config and secrets.

use crate::config::Delivery;
use crate::delivery::bdpan::{BdpanUploader, LoginStatus, LOGIN_HINT, NOT_LOGGED_IN};
use crate::models::{OrderStatus, PackageStatus};
use crate::package::validate::gitignore_covers_delivery;
use crate::repo::{orders as repo_orders, packages};
use crate::services::Ctx;
use crate::{secrets, templates, Error, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use time::{Duration, OffsetDateTime};

/// Warn when the bdpan login token expires sooner than this.
const BDPAN_TOKEN_WARN: Duration = Duration::days(7);

#[derive(Debug, Serialize)]
pub struct Report {
    pub problems: Vec<Problem>,
    /// Not wrong yet, but soon will be (e.g. a bdpan login about to expire).
    pub warnings: Vec<Problem>,
    pub fixed: Vec<String>,
    /// Orders migrated from v1 that are closed and have no directory on disk; expected, not checked.
    pub legacy_closed_without_dir: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Problem {
    pub scope: String,
    pub message: String,
}

fn problem(out: &mut Vec<Problem>, scope: impl Into<String>, message: impl Into<String>) {
    out.push(Problem {
        scope: scope.into(),
        message: message.into(),
    });
}

pub fn run(ctx: &Ctx, fix: bool) -> Result<Report> {
    let mut problems = Vec::new();
    let mut warnings = Vec::new();
    let mut fixed = Vec::new();
    let mut legacy_closed_without_dir = Vec::new();

    for order in repo_orders::list(&ctx.conn, true)? {
        let scope = format!("#{} {}", order.id, order.slug);
        // v1 archived projects were often deleted after archiving; nothing to repair.
        if order.legacy_id.is_some() && !order.status.is_active() {
            let root = order.archive_path.as_deref().or(order.dev_path.as_deref());
            if !root.map(Path::new).is_some_and(Path::is_dir) {
                legacy_closed_without_dir.push(scope);
                continue;
            }
        }
        let root = if order.status == OrderStatus::Archived {
            order.archive_path.as_deref()
        } else {
            order.dev_path.as_deref()
        };
        match root.map(Path::new) {
            Some(p) if p.is_dir() => {
                // Orders migrated from v1 predate the v2 file rules and are not retrofitted.
                if order.status.is_active() && order.legacy_id.is_none() {
                    for f in [".gig/JOB.md", ".gig/QUOTE.md"] {
                        if !p.join(f).is_file() {
                            problem(&mut problems, &scope, format!("missing {f}"));
                        }
                    }
                    if p.join("delivery").is_dir() && !gitignore_covers_delivery(p) {
                        problem(&mut problems, &scope, "delivery/ is not gitignored");
                    }
                }
            }
            Some(p) => {
                let candidates = [
                    ctx.config.general.dev_root.join(&order.slug),
                    ctx.config.general.archive_root.join(&order.slug),
                ];
                let found = candidates.iter().find(|c| c.is_dir());
                match (fix, found) {
                    (true, Some(c)) => {
                        let col = if order.status == OrderStatus::Archived {
                            "archive_path"
                        } else {
                            "dev_path"
                        };
                        repo_orders::set_text(
                            &ctx.conn,
                            order.id,
                            col,
                            Some(&c.to_string_lossy()),
                        )?;
                        fixed.push(format!("{scope}: {col} -> {}", c.display()));
                    }
                    _ => problem(
                        &mut problems,
                        &scope,
                        format!(
                            "directory {} is missing{}",
                            p.display(),
                            found
                                .map(|c| format!(" (found {}; run --fix)", c.display()))
                                .unwrap_or_default()
                        ),
                    ),
                }
            }
            None if order.status != OrderStatus::Cancelled && order.legacy_id.is_none() => {
                problem(&mut problems, &scope, "no directory recorded")
            }
            None => {}
        }
        for pkg in packages::list_for_order(&ctx.conn, order.id)? {
            if pkg.status == PackageStatus::Legacy {
                continue;
            }
            let zip = resolve_relative(&pkg.zip_path, root);
            if !zip.is_file() {
                problem(
                    &mut problems,
                    &scope,
                    format!("package {} zip missing: {}", pkg.package_id, zip.display()),
                );
            }
        }
    }

    let tdir = &ctx.config.general.templates_dir;
    let missing = templates::missing(tdir);
    if !missing.is_empty() {
        problem(
            &mut problems,
            "templates",
            format!("missing in {}: {}", tdir.display(), missing.join(", ")),
        );
    }
    for (name, p) in [
        ("dev_root", &ctx.config.general.dev_root),
        ("archive_root", &ctx.config.general.archive_root),
    ] {
        if !p.is_dir() {
            problem(
                &mut problems,
                "config",
                format!("{name} {} is not a directory", p.display()),
            );
        }
    }
    let uploader = ctx.config.delivery.uploader.as_str();
    if uploader == "bdpan" {
        check_bdpan(&ctx.config.delivery, &mut problems, &mut warnings);
    } else if ctx.config.s3_uploader_name().is_none() && !uploader.is_empty() {
        problem(
            &mut problems,
            "config",
            format!("delivery.uploader {uploader:?} is not s3:<name> or bdpan"),
        );
    }
    for m in secrets::availability(&ctx.config, &ctx.paths) {
        problem(&mut problems, "secrets", m);
    }
    if ctx.paths.legacy_db_file.is_file() {
        problem(
            &mut problems,
            "legacy",
            format!(
                "v1 database still present at {}; delete it once the migration is verified",
                ctx.paths.legacy_db_file.display()
            ),
        );
    }
    Ok(Report {
        problems,
        warnings,
        fixed,
        legacy_closed_without_dir,
    })
}

/// bdpan owns its login: ask `whoami` and report, never fix.
fn check_bdpan(delivery: &Delivery, problems: &mut Vec<Problem>, warnings: &mut Vec<Problem>) {
    match BdpanUploader::new(delivery).login_status() {
        // The message alone: the scope already says bdpan, and "upload
        // failed: ..." would misread a doctor check.
        Err(Error::Upload(m) | Error::Config(m) | Error::Secrets(m)) => {
            problem(problems, "bdpan", m)
        }
        Err(e) => problem(problems, "bdpan", e.to_string()),
        Ok(s) if !s.logged_in => problem(problems, "bdpan", NOT_LOGGED_IN),
        Ok(LoginStatus {
            expires_at: Some(t),
            ..
        }) if t - OffsetDateTime::now_utc() < BDPAN_TOKEN_WARN => problem(
            warnings,
            "bdpan",
            format!("the login token expires on {}; {LOGIN_HINT}", t.date()),
        ),
        Ok(_) => {}
    }
}

/// v1 stored some package paths relative to the project directory.
pub fn resolve_relative(path: &str, root: Option<&str>) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        root.map(|r| Path::new(r).join(&p)).unwrap_or(p)
    }
}
