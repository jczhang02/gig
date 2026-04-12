//! Rclone-based uploader backend.
//!
//! Shells out to the `rclone` binary:
//!   1. `rclone copyto <local> <remote>/<filename>`  — upload
//!   2. `rclone link  <remote>/<filename>`            — get shareable URL
//!
//! The user must have `rclone` installed and configured with the named remote.

use super::{UploadOpts, UploadResult, Uploader};
use crate::{Error, Result};
use std::path::Path;
use std::process::Command;

/// Uploader that delegates to the system `rclone` binary.
pub struct RcloneUploader {
    /// Rclone remote path, e.g. `"r2:gig-delivery"`.
    pub remote: String,
}

impl RcloneUploader {
    pub fn new(remote: impl Into<String>) -> Self {
        Self {
            remote: remote.into(),
        }
    }
}

impl Uploader for RcloneUploader {
    fn name(&self) -> &str {
        "rclone"
    }

    fn upload(&self, local: &Path, _opts: &UploadOpts) -> Result<UploadResult> {
        let filename = local
            .file_name()
            .ok_or_else(|| Error::Invalid("local path has no filename".into()))?
            .to_string_lossy();

        let remote_path = format!("{}/{}", self.remote, filename);

        // 1. Copy the file to the remote.
        let copy_status = Command::new("rclone")
            .args(["copyto", &local.to_string_lossy(), &remote_path])
            .status()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Error::Config(
                        "rclone is not installed or not on PATH; \
                         install it from https://rclone.org/downloads/"
                            .into(),
                    )
                } else {
                    Error::Io(e)
                }
            })?;

        if !copy_status.success() {
            return Err(Error::Invalid(format!(
                "rclone copyto failed with exit code {}",
                copy_status.code().unwrap_or(-1)
            )));
        }

        // 2. Generate a shareable link.
        let link_output = Command::new("rclone")
            .args(["link", &remote_path])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Error::Config("rclone is not installed or not on PATH".into())
                } else {
                    Error::Io(e)
                }
            })?;

        if !link_output.status.success() {
            let stderr = String::from_utf8_lossy(&link_output.stderr);
            return Err(Error::Invalid(format!("rclone link failed: {stderr}")));
        }

        let url = String::from_utf8_lossy(&link_output.stdout)
            .trim()
            .to_string();

        if url.is_empty() {
            return Err(Error::Invalid("rclone link returned an empty URL".into()));
        }

        Ok(UploadResult {
            url,
            expires_at: None,
            provider: format!("rclone:{}", self.remote),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// When rclone is not installed, upload should return a clear error.
    #[test]
    fn upload_returns_clear_error_when_rclone_missing() {
        // Override PATH to guarantee rclone is not found.
        let orig = std::env::var_os("PATH");
        std::env::set_var("PATH", "");

        let uploader = RcloneUploader::new("r2:gig-test");
        let dummy = PathBuf::from("/tmp/dummy-gig-test.zip");
        let result = uploader.upload(
            &dummy,
            &UploadOpts {
                link_ttl_days: None,
            },
        );

        // Restore PATH before any assertions that might panic.
        match orig {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }

        match result {
            Err(Error::Config(msg)) => {
                assert!(
                    msg.contains("rclone"),
                    "error message should mention rclone; got: {msg}"
                );
            }
            other => panic!("expected Config error, got {other:?}"),
        }
    }
}
