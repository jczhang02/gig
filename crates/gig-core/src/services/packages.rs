//! Package commands: build, check, upload, sent, ls.

use crate::context;
use crate::delivery::{UploadOpts, Uploader};
use crate::models::{Channel, Order, OrderStatus, Package, PackageKind, PackageStatus};
use crate::package::{build, validate, Layout};
use crate::repo::{events, orders as repo_orders, packages as repo};
use crate::services::{require_yes, Ctx};
use crate::{clock, Error, Result};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub package: Package,
    pub files: Vec<String>,
    pub zip_sha256: String,
    pub zip_size: u64,
    pub warnings: Vec<String>,
}

fn dev_path(order: &Order) -> Result<PathBuf> {
    if !order.status.is_active() {
        return Err(Error::InvalidState(format!("order is {}", order.status)));
    }
    order
        .dev_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| Error::InvalidState("order has no dev_path".into()))
}

fn record_checked(ctx: &Ctx, order: &Order, checked: validate::Checked) -> Result<CheckResult> {
    let now = clock::now();
    let existing = repo::find(
        &ctx.conn,
        order.id,
        checked
            .layout
            .package_dir
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
    )?;
    // A re-check of an already sent package keeps its sent record only if the zip is unchanged.
    let (status, sent_at, channel, uploader, remote_url, short_url, expires_at) = match &existing {
        Some(p)
            if p.status == PackageStatus::Sent
                && p.zip_sha256.as_deref() == Some(&checked.zip_sha256) =>
        {
            (
                PackageStatus::Sent,
                p.sent_at.clone(),
                p.channel,
                p.uploader.clone(),
                p.remote_url.clone(),
                p.short_url.clone(),
                p.expires_at.clone(),
            )
        }
        _ => (PackageStatus::Checked, None, None, None, None, None, None),
    };
    let package_id = checked
        .layout
        .package_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let package = repo::upsert(
        &ctx.conn,
        &repo::NewPackage {
            order_id: order.id,
            package_id: &package_id,
            kind: checked.kind,
            dir: &checked.layout.package_dir.to_string_lossy(),
            manifest_path: &checked.layout.manifest_path.to_string_lossy(),
            zip_path: &checked.layout.zip_path.to_string_lossy(),
            zip_sha256: Some(&checked.zip_sha256),
            file_count: Some(checked.files.len() as i64),
            status,
            checked_at: Some(&now),
            sent_at: sent_at.as_deref(),
            channel,
            uploader: uploader.as_deref(),
            remote_url: remote_url.as_deref(),
            short_url: short_url.as_deref(),
            expires_at: expires_at.as_deref(),
            created_at: existing
                .as_ref()
                .map(|p| p.created_at.as_str())
                .unwrap_or(&now),
        },
    )?;
    Ok(CheckResult {
        package,
        files: checked.files,
        zip_sha256: checked.zip_sha256,
        zip_size: checked.zip_size,
        warnings: checked.warnings,
    })
}

fn rejected(ctx: &Ctx, order: &Order, package_id: &str, err: Error) -> Error {
    // An empty package directory is a setup slip, not a safety rejection.
    let counts =
        matches!(&err, Error::UnsafePackage(m) if !m.contains("package directory is empty"));
    if counts {
        let _ = events::insert(
            &ctx.conn,
            order.id,
            events::CHECK_REJECTED,
            Some(&format!("{package_id}: {err}")),
            &clock::now(),
        );
    }
    err
}

pub fn build_package(
    ctx: &Ctx,
    key: Option<&str>,
    package_id: &str,
    kind: PackageKind,
    write_manifest: bool,
    client_named: &[String],
) -> Result<CheckResult> {
    let order = context::resolve_key_or_cwd(&ctx.conn, key)?;
    let dev = dev_path(&order)?;
    let checked = build::build(&dev, package_id, kind, write_manifest, client_named)
        .map_err(|e| rejected(ctx, &order, package_id, e))?;
    record_checked(ctx, &order, checked)
}

pub fn check(ctx: &Ctx, key: Option<&str>, package_id: &str) -> Result<CheckResult> {
    let order = context::resolve_key_or_cwd(&ctx.conn, key)?;
    let dev = dev_path(&order)?;
    let checked =
        validate::check(&dev, package_id).map_err(|e| rejected(ctx, &order, package_id, e))?;
    record_checked(ctx, &order, checked)
}

pub fn list(ctx: &Ctx, key: Option<&str>) -> Result<Vec<Package>> {
    let order = context::resolve_key_or_cwd(&ctx.conn, key)?;
    repo::list_for_order(&ctx.conn, order.id)
}

#[derive(Debug, Serialize)]
pub struct SendResult {
    pub package: Package,
    pub order_status: OrderStatus,
    pub url: Option<String>,
    pub short_url: Option<String>,
    pub expires_at: Option<String>,
    pub size: u64,
    pub dry_run: bool,
    pub warnings: Vec<String>,
}

/// Re-validate and confirm the zip is the one that was checked.
fn ready_to_send(
    ctx: &Ctx,
    order: &Order,
    package_id: &str,
) -> Result<(validate::Checked, Package)> {
    let dev = dev_path(order)?;
    let previous = repo::find(&ctx.conn, order.id, package_id)?
        .ok_or_else(|| Error::NeedsCheck(format!("run gig package check {package_id} first")))?;
    if previous.status == PackageStatus::Legacy {
        return Err(Error::NeedsCheck(format!(
            "{package_id} is a legacy record; build and check it again"
        )));
    }
    let checked =
        validate::check(&dev, package_id).map_err(|e| rejected(ctx, order, package_id, e))?;
    if previous.zip_sha256.as_deref() != Some(&checked.zip_sha256) {
        return Err(Error::NeedsCheck(format!(
            "{package_id}.zip changed since it was checked; run gig package check again"
        )));
    }
    match (order.status, checked.kind) {
        (OrderStatus::InProgress, _) | (OrderStatus::Paid, _) | (OrderStatus::Delivered, _) => {}
        (OrderStatus::Queued, _) => {
            return Err(Error::InvalidState(
                "order is queued; run gig start first".into(),
            ))
        }
        (s, _) => return Err(Error::InvalidState(format!("order is {s}"))),
    }
    Ok((checked, previous))
}

fn after_send(
    ctx: &Ctx,
    order: &Order,
    kind: PackageKind,
    package_id: &str,
    now: &str,
) -> Result<OrderStatus> {
    match kind {
        PackageKind::Preview => {
            events::insert(
                &ctx.conn,
                order.id,
                events::PREVIEW_SENT,
                Some(package_id),
                now,
            )?;
            Ok(order.status)
        }
        PackageKind::Full => match order.status {
            OrderStatus::InProgress | OrderStatus::Delivered => {
                repo_orders::set_text(&ctx.conn, order.id, "delivered_at", Some(now))?;
                repo_orders::set_status(&ctx.conn, order.id, OrderStatus::Delivered)?;
                Ok(OrderStatus::Delivered)
            }
            OrderStatus::Paid => {
                repo_orders::append_note(
                    &ctx.conn,
                    order.id,
                    now,
                    &format!("warranty revision: {package_id}"),
                )?;
                Ok(OrderStatus::Paid)
            }
            s => Ok(s),
        },
    }
}

/// Everything `upload` checks before touching the network. Lets the CLI fail on
/// a stale check before it even builds an uploader.
pub fn preflight(ctx: &Ctx, key: Option<&str>, package_id: &str) -> Result<()> {
    let order = context::resolve_key_or_cwd(&ctx.conn, key)?;
    ready_to_send(ctx, &order, package_id).map(|_| ())
}

pub fn object_key(order: &Order, package_id: &str, stamp: &str) -> String {
    format!("{}/{package_id}/{stamp}/{package_id}.zip", order.slug)
}

pub fn upload(
    ctx: &Ctx,
    key: Option<&str>,
    package_id: &str,
    yes: bool,
    uploader: &dyn Uploader,
) -> Result<SendResult> {
    let order = context::resolve_key_or_cwd(&ctx.conn, key)?;
    let (checked, previous) = ready_to_send(ctx, &order, package_id)?;
    if !yes {
        return Ok(SendResult {
            package: previous,
            order_status: order.status,
            url: None,
            short_url: None,
            expires_at: None,
            size: checked.zip_size,
            dry_run: true,
            warnings: checked.warnings,
        });
    }
    require_yes(yes, "upload package")?;
    let now = clock::now();
    let result = uploader.upload(
        &checked.layout.zip_path,
        &UploadOpts {
            object_key: Some(object_key(&order, package_id, &clock::now_compact())),
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
    repo::mark_sent(
        &ctx.conn,
        previous.id,
        &repo::Sent {
            sent_at: &now,
            channel: Channel::Oss,
            uploader: Some(uploader.name()),
            remote_url: Some(&result.url),
            short_url: result.short_url.as_deref(),
            expires_at: expires_at.as_deref(),
        },
    )?;
    let order_status = after_send(ctx, &order, checked.kind, package_id, &now)?;
    let package = repo::find(&ctx.conn, order.id, package_id)?.expect("just updated");
    Ok(SendResult {
        package,
        order_status,
        url: Some(result.url),
        short_url: result.short_url,
        expires_at,
        size: result.file_size,
        dry_run: false,
        warnings: checked.warnings,
    })
}

pub fn sent(
    ctx: &Ctx,
    key: Option<&str>,
    package_id: &str,
    channel: Channel,
    note: Option<&str>,
    yes: bool,
) -> Result<SendResult> {
    if channel == Channel::Oss {
        return Err(Error::InvalidInput(
            "use gig package upload for the oss channel".into(),
        ));
    }
    let order = context::resolve_key_or_cwd(&ctx.conn, key)?;
    let (checked, previous) = ready_to_send(ctx, &order, package_id)?;
    if !yes {
        return Ok(SendResult {
            package: previous,
            order_status: order.status,
            url: None,
            short_url: None,
            expires_at: None,
            size: checked.zip_size,
            dry_run: true,
            warnings: checked.warnings,
        });
    }
    require_yes(yes, "record package as sent")?;
    let now = clock::now();
    repo::mark_sent(
        &ctx.conn,
        previous.id,
        &repo::Sent {
            sent_at: &now,
            channel,
            uploader: None,
            remote_url: None,
            short_url: None,
            expires_at: None,
        },
    )?;
    if let Some(n) = note {
        repo_orders::append_note(
            &ctx.conn,
            order.id,
            &now,
            &format!("sent {package_id} via {channel}: {n}"),
        )?;
    }
    let order_status = after_send(ctx, &order, checked.kind, package_id, &now)?;
    let package = repo::find(&ctx.conn, order.id, package_id)?.expect("just updated");
    Ok(SendResult {
        package,
        order_status,
        url: None,
        short_url: None,
        expires_at: None,
        size: checked.zip_size,
        dry_run: false,
        warnings: checked.warnings,
    })
}

pub fn layout_for(order: &Order, package_id: &str) -> Result<Layout> {
    Ok(Layout::new(&dev_path(order)?, package_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::UploadResult;
    use crate::services::orders::{self, tests::make};
    use std::path::Path;

    struct FakeUploader;
    impl Uploader for FakeUploader {
        fn name(&self) -> &str {
            "s3:fake"
        }
        fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
            assert!(opts.object_key.as_deref().unwrap().ends_with(".zip"));
            Ok(UploadResult {
                url: format!(
                    "https://x.test/{}",
                    local.file_name().unwrap().to_string_lossy()
                ),
                short_url: Some("https://go.test/abc".into()),
                expires_at: Some(1_900_000_000),
                provider: "s3:fake".into(),
                file_size: 3,
            })
        }
    }

    fn with_package(ctx: &Ctx, slug: &str, pkg: &str) -> Order {
        let o = make(ctx, slug);
        orders::start(ctx, Some(slug)).unwrap();
        let dir = Path::new(o.dev_path.as_ref().unwrap())
            .join("delivery")
            .join(pkg);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manual.pdf"), "pdf").unwrap();
        o
    }

    #[test]
    fn build_check_upload_flow() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        with_package(&ctx, "flow", "flow-v1.0.0");
        assert_eq!(
            upload(&ctx, Some("flow"), "flow-v1.0.0", true, &FakeUploader)
                .unwrap_err()
                .code(),
            "needs_check"
        );
        let c = build_package(
            &ctx,
            Some("flow"),
            "flow-v1.0.0",
            PackageKind::Full,
            true,
            &[],
        )
        .unwrap();
        assert_eq!(c.package.status, PackageStatus::Checked);
        assert_eq!(c.files, vec!["manual.pdf"]);
        let dry = upload(&ctx, Some("flow"), "flow-v1.0.0", false, &FakeUploader).unwrap();
        assert!(dry.dry_run);
        assert_eq!(dry.package.status, PackageStatus::Checked);
        let sent = upload(&ctx, Some("flow"), "flow-v1.0.0", true, &FakeUploader).unwrap();
        assert_eq!(sent.package.status, PackageStatus::Sent);
        assert_eq!(sent.package.channel, Some(Channel::Oss));
        assert_eq!(sent.short_url.as_deref(), Some("https://go.test/abc"));
        assert_eq!(sent.order_status, OrderStatus::Delivered);
        // re-check keeps the sent record when the zip is unchanged
        let again = check(&ctx, Some("flow"), "flow-v1.0.0").unwrap();
        assert_eq!(again.package.status, PackageStatus::Sent);
    }

    #[test]
    fn changed_zip_needs_new_check() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let o = with_package(&ctx, "chg", "chg-v1");
        build_package(&ctx, Some("chg"), "chg-v1", PackageKind::Full, true, &[]).unwrap();
        let dir = Path::new(o.dev_path.as_ref().unwrap()).join("delivery/chg-v1");
        std::fs::write(dir.join("manual.pdf"), "changed").unwrap();
        // rebuild zip without recording a check: emulate by building then restoring old sha in db
        build::build(
            dir.parent().unwrap().parent().unwrap(),
            "chg-v1",
            PackageKind::Full,
            false,
            &[],
        )
        .unwrap();
        let e = upload(&ctx, Some("chg"), "chg-v1", true, &FakeUploader).unwrap_err();
        assert_eq!(e.code(), "needs_check");
    }

    #[test]
    fn preview_does_not_change_status_and_records_event() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        with_package(&ctx, "pv", "pv-preview-1");
        build_package(
            &ctx,
            Some("pv"),
            "pv-preview-1",
            PackageKind::Preview,
            true,
            &[],
        )
        .unwrap();
        let s = sent(
            &ctx,
            Some("pv"),
            "pv-preview-1",
            Channel::Phone,
            Some("via gsconnect"),
            true,
        )
        .unwrap();
        assert_eq!(s.order_status, OrderStatus::InProgress);
        assert_eq!(s.package.channel, Some(Channel::Phone));
        assert_eq!(
            events::count(&ctx.conn, s.package.order_id, events::PREVIEW_SENT).unwrap(),
            1
        );
        assert!(sent(&ctx, Some("pv"), "pv-preview-1", Channel::Oss, None, true).is_err());
    }

    #[test]
    fn unsafe_package_is_recorded_as_rejection() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let o = with_package(&ctx, "bad", "bad-v1");
        let dir = Path::new(o.dev_path.as_ref().unwrap()).join("delivery/bad-v1");
        std::fs::write(dir.join("id_rsa"), "k").unwrap();
        let e =
            build_package(&ctx, Some("bad"), "bad-v1", PackageKind::Full, true, &[]).unwrap_err();
        assert_eq!(e.code(), "unsafe_package");
        assert_eq!(
            events::count(&ctx.conn, o.id, events::CHECK_REJECTED).unwrap(),
            1
        );
        assert!(repo::find(&ctx.conn, o.id, "bad-v1").unwrap().is_none());
    }

    #[test]
    fn warranty_work_keeps_paid_status() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let o = with_package(&ctx, "wr", "wr-v1");
        build_package(&ctx, Some("wr"), "wr-v1", PackageKind::Full, true, &[]).unwrap();
        sent(&ctx, Some("wr"), "wr-v1", Channel::Phone, None, true).unwrap();
        orders::paid(&ctx, Some("wr"), None, None).unwrap();
        let dir = Path::new(o.dev_path.as_ref().unwrap()).join("delivery/wr-v1.1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manual.pdf"), "fixed").unwrap();
        build_package(&ctx, Some("wr"), "wr-v1.1", PackageKind::Full, true, &[]).unwrap();
        let s = sent(&ctx, Some("wr"), "wr-v1.1", Channel::Phone, None, true).unwrap();
        assert_eq!(s.order_status, OrderStatus::Paid);
        let shown = orders::show(&ctx, Some("wr")).unwrap();
        assert!(shown.order.notes.contains("warranty revision: wr-v1.1"));
        assert_eq!(shown.packages.len(), 2);
    }
}
