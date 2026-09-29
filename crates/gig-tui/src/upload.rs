//! Package and artifact uploads (spec 2.1 `u`, `m`, `U`): which packages can
//! go out, what the order becomes, and the blocking upload on a worker thread
//! that feeds a progress bar.
//!
//! The service functions build their own `UploadOpts`, so the progress
//! callback rides in on a decorating `Uploader` instead of a new argument.

use crate::data::OrderRow;
use gig_core::delivery::{Progress, UploadOpts, UploadResult, Uploader};
use gig_core::models::{OrderStatus, Package, PackageKind, PackageStatus};
use gig_core::services::{artifacts, packages, Ctx};
use gig_core::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

/// One upload the app runs after a confirmed popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadJob {
    /// `packages::upload(yes=true)`.
    Package { slug: String, package_id: String },
    /// `artifacts::upload(yes=true)`.
    Artifact { slug: String, path: PathBuf },
}

impl UploadJob {
    pub fn title(&self) -> String {
        match self {
            UploadJob::Package { package_id, .. } => format!("uploading {package_id}"),
            UploadJob::Artifact { path, .. } => format!(
                "uploading {}",
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into()
                )
            ),
        }
    }
}

/// A finished upload, for the result popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uploaded {
    /// Package id or file name.
    pub what: String,
    /// Order status after the upload; `None` for artifacts, which never
    /// change it.
    pub order_status: Option<OrderStatus>,
    pub size: u64,
    pub url: Option<String>,
    pub short_url: Option<String>,
    pub expires_at: Option<String>,
    pub warnings: Vec<String>,
}

impl Uploaded {
    /// The link to hand the client: the short one when there is one.
    pub fn link(&self) -> Option<&str> {
        self.short_url.as_deref().or(self.url.as_deref())
    }

    /// Result popup body; the app adds the clipboard outcome.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "uploaded {} ({})",
            self.what,
            human_size(self.size)
        )];
        if let Some(s) = self.order_status {
            lines.push(format!("order: {s}"));
        }
        if let Some(e) = &self.expires_at {
            lines.push(format!("link expires: {e}"));
        }
        for w in &self.warnings {
            lines.push(format!("warning: {w}"));
        }
        if self.link().is_none() {
            lines.push("no link returned".into());
        }
        lines
    }
}

/// Packages of the order that can go out: checked and not yet sent.
pub fn checked_packages(row: &OrderRow) -> Vec<&Package> {
    row.packages
        .iter()
        .filter(|p| p.status == PackageStatus::Checked)
        .collect()
}

/// Order status after a package goes out, as gig-core's `after_send` sets
/// it: a preview changes nothing; a full package delivers an order in
/// progress (or re-delivers a delivered one) and keeps a paid one paid (a
/// warranty revision).
pub fn state_after(status: OrderStatus, kind: PackageKind) -> OrderStatus {
    match (kind, status) {
        (PackageKind::Full, OrderStatus::InProgress | OrderStatus::Delivered) => {
            OrderStatus::Delivered
        }
        _ => status,
    }
}

/// `1.2 MB` style sizes for popups.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64 / 1024.0;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit])
}

/// Expand a leading `~/` in a typed path.
pub fn expand_home(typed: &str) -> PathBuf {
    let typed = typed.trim();
    match (typed.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest),
        _ => PathBuf::from(typed),
    }
}

/// Stand-in for dry runs (`yes=false`), which validate but never upload.
pub struct NoUploader;

impl Uploader for NoUploader {
    fn name(&self) -> &str {
        "none"
    }

    fn upload(&self, _local: &Path, _opts: &UploadOpts) -> Result<UploadResult> {
        Err(Error::Upload("no uploader in a dry run".into()))
    }
}

/// Bytes sent and total, written by the uploader's callback on the worker
/// thread and read by the drawing thread. Total 0 means no report yet.
#[derive(Debug, Default)]
pub struct Meter {
    sent: AtomicU64,
    total: AtomicU64,
}

impl Meter {
    pub fn callback(self: &Arc<Self>) -> Progress {
        let meter = Arc::clone(self);
        Arc::new(move |sent, total| {
            meter.total.store(total, Ordering::Relaxed);
            meter.sent.store(sent, Ordering::Relaxed);
        })
    }

    /// `(sent, total)`.
    pub fn get(&self) -> (u64, u64) {
        (
            self.sent.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
}

/// Puts a progress callback into the options the service builds.
struct WithProgress<'a> {
    inner: &'a dyn Uploader,
    progress: Progress,
}

impl Uploader for WithProgress<'_> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
        let mut opts = opts.clone();
        opts.progress = Some(Arc::clone(&self.progress));
        self.inner.upload(local, &opts)
    }
}

/// Run `job` on this thread through the same service call as the CLI.
pub fn run_job(
    ctx: &Ctx,
    job: &UploadJob,
    uploader: &dyn Uploader,
    progress: Option<Progress>,
) -> Result<Uploaded> {
    let wrapped;
    let uploader: &dyn Uploader = match progress {
        Some(progress) => {
            wrapped = WithProgress {
                inner: uploader,
                progress,
            };
            &wrapped
        }
        None => uploader,
    };
    match job {
        UploadJob::Package { slug, package_id } => {
            let r = packages::upload(ctx, Some(slug), package_id, true, uploader)?;
            Ok(Uploaded {
                what: package_id.clone(),
                order_status: Some(r.order_status),
                size: r.size,
                url: r.url,
                short_url: r.short_url,
                expires_at: r.expires_at,
                warnings: r.warnings,
            })
        }
        UploadJob::Artifact { slug, path } => {
            let r = artifacts::upload(ctx, Some(slug), path, true, uploader)?;
            Ok(Uploaded {
                what: path
                    .file_name()
                    .map_or_else(|| r.local_path.clone(), |n| n.to_string_lossy().into()),
                order_status: None,
                size: r.size,
                url: r.url,
                short_url: r.short_url,
                expires_at: r.artifact.and_then(|a| a.expires_at),
                warnings: Vec::new(),
            })
        }
    }
}

/// Run `job` on a worker thread and call `tick` with `(sent, total)` until
/// it finishes. `tick` paces the loop (the app draws and waits for input
/// there). The worker borrows the database handle for the duration, which is
/// why this takes `&mut Ctx`: a connection may move between threads but not
/// be shared.
pub fn run_on_worker(
    ctx: &mut Ctx,
    job: &UploadJob,
    uploader: &dyn Uploader,
    mut tick: impl FnMut(u64, u64),
) -> Result<Uploaded> {
    let meter = Arc::new(Meter::default());
    let progress = meter.callback();
    // A message left by an earlier worker must not be reported for this one.
    let _ = crate::terminal::take_worker_panic();
    thread::scope(|s| {
        let worker = thread::Builder::new()
            .name(crate::terminal::WORKER_THREAD.into())
            .spawn_scoped(s, move || run_job(ctx, job, uploader, Some(progress)))
            .map_err(|e| Error::Upload(format!("cannot start the upload worker: {e}")))?;
        while !worker.is_finished() {
            let (sent, total) = meter.get();
            tick(sent, total);
        }
        worker.join().unwrap_or_else(|payload| {
            let what = crate::terminal::take_worker_panic()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            Err(Error::Upload(format!("upload worker panicked: {what}")))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resulting_states() {
        use OrderStatus::*;
        assert_eq!(state_after(InProgress, PackageKind::Full), Delivered);
        assert_eq!(state_after(Delivered, PackageKind::Full), Delivered);
        assert_eq!(state_after(Paid, PackageKind::Full), Paid);
        assert_eq!(state_after(InProgress, PackageKind::Preview), InProgress);
        assert_eq!(state_after(Paid, PackageKind::Preview), Paid);
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(812), "812 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(120 * 1024 * 1024), "120.0 MB");
    }

    #[test]
    fn meter_records_the_last_report() {
        let m = Arc::new(Meter::default());
        assert_eq!(m.get(), (0, 0));
        let cb = m.callback();
        cb(8, 20);
        cb(16, 20);
        assert_eq!(m.get(), (16, 20));
    }

    #[test]
    fn job_titles() {
        let p = UploadJob::Package {
            slug: "a".into(),
            package_id: "a-v1".into(),
        };
        assert_eq!(p.title(), "uploading a-v1");
        let a = UploadJob::Artifact {
            slug: "a".into(),
            path: "/tmp/x/report.pdf".into(),
        };
        assert_eq!(a.title(), "uploading report.pdf");
    }
}
