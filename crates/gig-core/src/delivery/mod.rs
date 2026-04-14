//! Delivery trait and uploader backends.
//!
//! The `Uploader` trait is the only interface that `gig deliver` calls.
//! Backends live in submodules.

pub mod s3;

pub use s3::S3Uploader;

use crate::Result;
use std::path::Path;

/// Options passed to every uploader.
#[derive(Debug, Clone)]
pub struct UploadOpts {
    /// If set, request that the shared link expires after this many days.
    /// Not all backends honour this; it is advisory.
    pub link_ttl_days: Option<u32>,
}

/// Result returned by a successful upload.
#[derive(Debug, Clone)]
pub struct UploadResult {
    /// Shareable URL.
    pub url: String,
    /// Expiry as unix seconds, if the backend provided one.
    pub expires_at: Option<i64>,
    /// Human-readable provider identifier (e.g. "rclone:r2:gig-delivery").
    pub provider: String,
}

/// The pluggable uploader interface.
pub trait Uploader: Send + Sync {
    /// Short, stable name used in config and artifact records (e.g. "rclone").
    fn name(&self) -> &str;

    /// Upload `local` file to the backend and return a shareable link.
    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult>;
}
