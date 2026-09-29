//! Single files shared outside a package.

use crate::context;
use crate::delivery::{UploadOpts, Uploader};
use crate::models::Artifact;
use crate::package::rules;
use crate::repo::artifacts as repo;
use crate::services::{require_yes, Ctx};
use crate::{clock, Error, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ArtifactUpload {
    pub artifact: Option<Artifact>,
    pub local_path: String,
    pub size: u64,
    pub url: Option<String>,
    pub short_url: Option<String>,
    pub dry_run: bool,
}

pub fn upload(
    ctx: &Ctx,
    key: Option<&str>,
    file: &Path,
    yes: bool,
    uploader: &dyn Uploader,
) -> Result<ArtifactUpload> {
    let order = context::resolve_key_or_cwd(&ctx.conn, key)?;
    if !order.status.is_active() {
        return Err(Error::InvalidState(format!("order is {}", order.status)));
    }
    let meta = std::fs::symlink_metadata(file)
        .map_err(|e| Error::PathUnavailable(file.to_path_buf(), e))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(Error::UnsafePackage(format!(
            "{} is not a regular file",
            file.display()
        )));
    }
    let name = file
        .file_name()
        .ok_or_else(|| Error::InvalidInput("file has no name".into()))?
        .to_string_lossy()
        .into_owned();
    rules::check_path(&name, &[])?;
    let local_path = std::fs::canonicalize(file)?.to_string_lossy().into_owned();
    if !yes {
        return Ok(ArtifactUpload {
            artifact: None,
            local_path,
            size: meta.len(),
            url: None,
            short_url: None,
            dry_run: true,
        });
    }
    require_yes(yes, "upload artifact")?;
    let now = clock::now();
    let result = uploader.upload(
        file,
        &UploadOpts {
            object_key: Some(format!(
                "{}/artifacts/{}/{name}",
                order.slug,
                clock::now_compact()
            )),
            progress: None,
        },
    )?;
    let expires_at = result
        .expires_at
        .and_then(|s| time::OffsetDateTime::from_unix_timestamp(s).ok())
        .map(|t| {
            t.replace_nanosecond(0)
                .unwrap()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap()
        });
    let artifact = repo::insert(
        &ctx.conn,
        &repo::NewArtifact {
            order_id: order.id,
            local_path: Some(&local_path),
            uploader: Some(uploader.name()),
            remote_url: Some(&result.url),
            short_url: result.short_url.as_deref(),
            expires_at: expires_at.as_deref(),
            uploaded_at: &now,
        },
    )?;
    Ok(ArtifactUpload {
        artifact: Some(artifact),
        local_path,
        size: result.file_size,
        url: Some(result.url),
        short_url: result.short_url,
        dry_run: false,
    })
}

pub fn list(ctx: &Ctx, key: Option<&str>) -> Result<Vec<Artifact>> {
    let order = context::resolve_key_or_cwd(&ctx.conn, key)?;
    repo::list_for_order(&ctx.conn, order.id)
}
