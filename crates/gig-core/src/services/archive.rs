//! Archive: preview the dirty state, then move (or purge) the project directory.

use crate::context;
use crate::models::{Order, OrderStatus, PackageStatus};
use crate::repo::{orders as repo_orders, packages, scorecards};
use crate::services::{require_yes, Ctx};
use crate::{clock, Error, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const LARGE_FILE_BYTES: u64 = 50 * 1024 * 1024;

pub struct ArchiveOptions {
    pub yes: bool,
    pub before_warranty_end: bool,
    pub no_scorecard: bool,
    pub purge: bool,
}

#[derive(Debug, Serialize)]
pub struct ArchiveReport {
    pub order: Order,
    pub dry_run: bool,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub purge: bool,
    pub git_dirty: Vec<String>,
    pub large_files: Vec<LargeFile>,
    pub unsent_packages: Vec<String>,
    pub missing_scorecard: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LargeFile {
    pub path: String,
    pub bytes: u64,
}

pub fn archive(ctx: &Ctx, key: Option<&str>, opts: &ArchiveOptions) -> Result<ArchiveReport> {
    let order = context::resolve_key_or_cwd(&ctx.conn, key)?;
    let today = clock::today();
    let mut blockers = Vec::new();
    match order.status {
        OrderStatus::Paid => {
            if let Some(until) = &order.warranty_until {
                if until.as_str() > today.as_str() && !opts.before_warranty_end {
                    blockers.push(format!(
                        "warranty runs until {until}; pass --before-warranty-end to archive anyway"
                    ));
                }
            }
        }
        OrderStatus::Cancelled => {}
        s => blockers.push(format!("archive needs paid or cancelled, order is {s}")),
    }
    let missing_scorecard =
        scorecards::find(&ctx.conn, order.id)?.is_none() && order.status == OrderStatus::Paid;
    if missing_scorecard && !opts.no_scorecard {
        blockers.push("no scorecard; run gig scorecard first or pass --no-scorecard".into());
    }
    let source = order.dev_path.as_deref().map(PathBuf::from);
    let destination = ctx.config.general.archive_root.join(&order.slug);
    let mut git_dirty = Vec::new();
    let mut large_files = Vec::new();
    match &source {
        Some(src) if src.is_dir() => {
            git_dirty = git_status(src);
            walk_large(src, src, &mut large_files)?;
        }
        Some(src) => blockers.push(format!(
            "project directory {} does not exist",
            src.display()
        )),
        None => blockers.push("order has no dev_path".into()),
    }
    if !opts.purge && destination.exists() {
        blockers.push(format!("{} already exists", destination.display()));
    }
    let unsent_packages: Vec<String> = packages::list_for_order(&ctx.conn, order.id)?
        .into_iter()
        .filter(|p| p.status != PackageStatus::Sent)
        .map(|p| p.package_id)
        .collect();

    let report = |order: Order, dry_run: bool| ArchiveReport {
        source: source.as_ref().map(|p| p.to_string_lossy().into_owned()),
        destination: (!opts.purge).then(|| destination.to_string_lossy().into_owned()),
        purge: opts.purge,
        git_dirty: git_dirty.clone(),
        large_files: large_files
            .iter()
            .map(|(p, b)| LargeFile {
                path: p.clone(),
                bytes: *b,
            })
            .collect(),
        unsent_packages: unsent_packages.clone(),
        missing_scorecard,
        blockers: blockers.clone(),
        order,
        dry_run,
    };

    if !opts.yes || !blockers.is_empty() {
        if opts.yes {
            return Err(Error::InvalidState(blockers.join("; ")));
        }
        return Ok(report(order, true));
    }
    require_yes(opts.yes, "archive")?;
    let src = source.clone().expect("checked above");
    let now = clock::now();
    if opts.purge {
        std::fs::remove_dir_all(&src).map_err(|e| Error::PathUnavailable(src.clone(), e))?;
        repo_orders::append_note(
            &ctx.conn,
            order.id,
            &now,
            "project directory purged at archive",
        )?;
    } else {
        std::fs::create_dir_all(&ctx.config.general.archive_root)
            .map_err(|e| Error::PathUnavailable(ctx.config.general.archive_root.clone(), e))?;
        move_dir(&src, &destination)?;
        repo_orders::set_text(
            &ctx.conn,
            order.id,
            "archive_path",
            Some(&destination.to_string_lossy()),
        )?;
    }
    repo_orders::set_text(&ctx.conn, order.id, "archived_at", Some(&now))?;
    repo_orders::set_status(&ctx.conn, order.id, OrderStatus::Archived)?;
    let order = repo_orders::find_by_id(&ctx.conn, order.id)?;
    Ok(report(order, false))
}

fn move_dir(src: &Path, dst: &Path) -> Result<()> {
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(18) => {
            // EXDEV: cross-device; copy then remove.
            copy_dir(src, dst)?;
            std::fs::remove_dir_all(src).map_err(|e| Error::PathUnavailable(src.to_path_buf(), e))
        }
        Err(e) => Err(Error::PathUnavailable(dst.to_path_buf(), e)),
    }
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        let meta = entry.metadata()?;
        if meta.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if meta.file_type().is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(entry.path())?, &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn git_status(dir: &Path) -> Vec<String> {
    let out = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "status", "--porcelain"])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.to_string())
            .collect(),
        _ => vec!["(git status unavailable)".into()],
    }
}

fn walk_large(root: &Path, dir: &Path, out: &mut Vec<(String, u64)>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            walk_large(root, &path, out)?;
        } else if meta.len() >= LARGE_FILE_BYTES {
            out.push((
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned(),
                meta.len(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::orders::{self, tests::make, ScorecardInput};

    fn opts(yes: bool) -> ArchiveOptions {
        ArchiveOptions {
            yes,
            before_warranty_end: false,
            no_scorecard: false,
            purge: false,
        }
    }

    #[test]
    fn archive_moves_after_warranty_and_scorecard() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let o = make(&ctx, "ar");
        orders::start(&ctx, Some("ar")).unwrap();
        repo_orders::set_status(&ctx.conn, o.id, OrderStatus::Delivered).unwrap();
        orders::paid(&ctx, Some("ar"), Some("2020-01-01"), None).unwrap();

        let r = archive(&ctx, Some("ar"), &opts(false)).unwrap();
        assert!(r.dry_run);
        assert!(r.blockers.iter().any(|b| b.contains("scorecard")));
        assert_eq!(
            archive(&ctx, Some("ar"), &opts(true)).unwrap_err().code(),
            "invalid_state"
        );

        orders::scorecard(
            &ctx,
            Some("ar"),
            &ScorecardInput {
                decisions: 1,
                repeat_questions: 0,
                cleanups: 0,
                report_reworks: 0,
                score: 5,
                note: None,
            },
        )
        .unwrap();
        let r = archive(&ctx, Some("ar"), &opts(true)).unwrap();
        assert!(!r.dry_run);
        assert_eq!(r.order.status, OrderStatus::Archived);
        let dest = root.path().join("archive/ar");
        assert!(dest.join(".gig/JOB.md").is_file());
        assert!(!root.path().join("dev/ar").exists());
        assert_eq!(
            orders::cd(&ctx, Some("ar")).unwrap(),
            dest.to_string_lossy()
        );
    }

    #[test]
    fn warranty_blocks_until_flag() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let o = make(&ctx, "wb");
        repo_orders::set_status(&ctx.conn, o.id, OrderStatus::Delivered).unwrap();
        orders::paid(&ctx, Some("wb"), None, None).unwrap();
        let r = archive(
            &ctx,
            Some("wb"),
            &ArchiveOptions {
                yes: false,
                before_warranty_end: false,
                no_scorecard: true,
                purge: false,
            },
        )
        .unwrap();
        assert!(r.blockers.iter().any(|b| b.contains("warranty")));
        let r = archive(
            &ctx,
            Some("wb"),
            &ArchiveOptions {
                yes: true,
                before_warranty_end: true,
                no_scorecard: true,
                purge: true,
            },
        )
        .unwrap();
        assert!(r.purge);
        assert!(!root.path().join("dev/wb").exists());
        assert!(r.order.archive_path.is_none());
    }

    #[test]
    fn cancelled_orders_can_be_archived() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        make(&ctx, "cx");
        orders::cancel(&ctx, Some("cx"), "gone", true).unwrap();
        let r = archive(
            &ctx,
            Some("cx"),
            &ArchiveOptions {
                yes: true,
                before_warranty_end: false,
                no_scorecard: false,
                purge: false,
            },
        )
        .unwrap();
        assert_eq!(r.order.status, OrderStatus::Archived);
    }
}
