//! Pre-order drafts: a notes file in a hidden directory, and a row that outlives it.

use crate::models::{Draft, DraftStatus, ProjectType};
use crate::repo::drafts as repo;
use crate::services::{require_yes, validate_slug, Ctx};
use crate::{clock, templates, Error, Result};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct DraftCreated {
    pub draft: Draft,
    pub notes_path: PathBuf,
}

pub fn new(
    ctx: &Ctx,
    slug: &str,
    title: Option<&str>,
    material_path: Option<&str>,
    project_type: Option<ProjectType>,
) -> Result<DraftCreated> {
    validate_slug(slug)?;
    if crate::repo::orders::find_by_slug(&ctx.conn, slug)?.is_some() {
        return Err(Error::InvalidInput(format!(
            "an order named {slug} already exists"
        )));
    }
    let dir = ctx.config.general.drafts_dir().join(slug);
    if dir.exists() {
        return Err(Error::InvalidInput(format!(
            "{} already exists",
            dir.display()
        )));
    }
    let tdir = &ctx.config.general.templates_dir;
    let text = templates::render(
        tdir,
        "NOTES.md.j2",
        &serde_json::json!({
            "slug": slug,
            "title": title,
            "material_path": material_path,
            "project_type": project_type.map(|t| t.as_str()),
            "today": clock::today(),
        }),
    )?;
    std::fs::create_dir_all(&dir).map_err(|e| Error::PathUnavailable(dir.clone(), e))?;
    let notes_path = dir.join("NOTES.md");
    std::fs::write(&notes_path, text).map_err(|e| Error::PathUnavailable(notes_path.clone(), e))?;
    let now = clock::now();
    let draft = repo::insert(
        &ctx.conn,
        &repo::NewDraft {
            slug,
            title,
            material_path,
            project_type,
            notes_dir: &dir.to_string_lossy(),
            status: DraftStatus::Open,
            drop_reason: None,
            notes_snapshot: None,
            promoted_order_id: None,
            created_at: &now,
            closed_at: None,
        },
    )?;
    Ok(DraftCreated { draft, notes_path })
}

pub fn list(ctx: &Ctx, all: bool) -> Result<Vec<Draft>> {
    repo::list(&ctx.conn, all)
}

pub fn open_draft(ctx: &Ctx, slug: &str) -> Result<Draft> {
    let d = repo::find_by_slug(&ctx.conn, slug)?
        .ok_or_else(|| Error::NotFound(format!("draft {slug}")))?;
    if d.status != DraftStatus::Open {
        return Err(Error::InvalidState(format!("draft {slug} is {}", d.status)));
    }
    Ok(d)
}

/// The notes text on disk, if the directory still exists.
pub fn read_notes(draft: &Draft) -> Option<String> {
    std::fs::read_to_string(PathBuf::from(&draft.notes_dir).join("NOTES.md")).ok()
}

#[derive(Debug, Serialize)]
pub struct DropPreview {
    pub draft: Draft,
    pub would_delete: Vec<String>,
    pub dry_run: bool,
}

pub fn drop(ctx: &Ctx, slug: &str, reason: &str, yes: bool) -> Result<DropPreview> {
    let d = open_draft(ctx, slug)?;
    let dir = PathBuf::from(&d.notes_dir);
    let mut would_delete = Vec::new();
    if dir.exists() {
        collect(&dir, &mut would_delete)?;
    }
    if !yes {
        return Ok(DropPreview {
            draft: d,
            would_delete,
            dry_run: true,
        });
    }
    require_yes(yes, "drop draft")?;
    let snapshot = read_notes(&d);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| Error::PathUnavailable(dir.clone(), e))?;
    }
    let d = repo::close(
        &ctx.conn,
        d.id,
        DraftStatus::Dropped,
        Some(reason),
        snapshot.as_deref(),
        None,
        &clock::now(),
    )?;
    Ok(DropPreview {
        draft: d,
        would_delete,
        dry_run: false,
    })
}

/// Called by `orders::new --from-draft`: snapshot notes, remove the directory, mark promoted.
pub fn promote(ctx: &Ctx, draft: &Draft, order_id: i64) -> Result<Draft> {
    let snapshot = read_notes(draft);
    let dir = PathBuf::from(&draft.notes_dir);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| Error::PathUnavailable(dir.clone(), e))?;
    }
    repo::close(
        &ctx.conn,
        draft.id,
        DraftStatus::Promoted,
        None,
        snapshot.as_deref(),
        Some(order_id),
        &clock::now(),
    )
}

fn collect(dir: &std::path::Path, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            collect(&p, out)?;
        } else {
            out.push(p.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_lifecycle() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let c = new(&ctx, "pdf-rename", Some("PDF"), Some("/mnt/x"), None).unwrap();
        assert!(c.notes_path.is_file());
        assert!(std::fs::read_to_string(&c.notes_path)
            .unwrap()
            .contains("/mnt/x"));
        assert_eq!(list(&ctx, false).unwrap().len(), 1);
        assert!(new(&ctx, "pdf-rename", None, None, None).is_err());

        let preview = drop(&ctx, "pdf-rename", "no budget", false).unwrap();
        assert!(preview.dry_run);
        assert_eq!(preview.would_delete.len(), 1);
        assert!(c.notes_path.is_file());

        let done = drop(&ctx, "pdf-rename", "no budget", true).unwrap();
        assert!(!done.dry_run);
        assert_eq!(done.draft.status, DraftStatus::Dropped);
        assert!(done.draft.notes_snapshot.unwrap().contains("/mnt/x"));
        assert!(!c.notes_path.exists());
        assert!(list(&ctx, false).unwrap().is_empty());
        assert_eq!(
            drop(&ctx, "pdf-rename", "x", true).unwrap_err().code(),
            "invalid_state"
        );
    }
}
