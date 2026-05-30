//! S3-compatible uploader using the official AWS SDK for Rust.
//!
//! Works with AWS S3, Alibaba Cloud OSS, Cloudflare R2, MinIO,
//! and any S3-compatible service.

use super::{UploadOpts, UploadResult, Uploader};
use crate::config::S3UploaderConfig;
use crate::{Error, Result};
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::Client;
use std::io::Read;
use std::path::Path;
use std::time::Duration;
use time::OffsetDateTime;

/// Files at or above this size use multipart upload.
const MULTIPART_THRESHOLD: u64 = 100 * 1024 * 1024; // 100 MB
/// Size of each part in a multipart upload.
const PART_SIZE: usize = 8 * 1024 * 1024; // 8 MB

/// Uploader that talks to any S3-compatible API using the official AWS SDK.
pub struct S3Uploader {
    name: String,
    client: Client,
    download_client: Client,
    rt: tokio::runtime::Runtime,
    bucket: String,
    link_ttl_seconds: u32,
}

impl S3Uploader {
    /// Create a new S3Uploader from config.
    pub fn new(name: String, cfg: &S3UploaderConfig) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::Invalid(format!("failed to create tokio runtime: {e}")))?;

        let client = make_client(cfg, &cfg.endpoint);
        let download_endpoint = cfg.download_endpoint.as_deref().unwrap_or(&cfg.endpoint);
        let download_client = make_client(cfg, download_endpoint);

        Ok(Self {
            name,
            client,
            download_client,
            rt,
            bucket: cfg.bucket.clone(),
            link_ttl_seconds: cfg.link_ttl_seconds,
        })
    }
}

fn make_client(cfg: &S3UploaderConfig, endpoint: &str) -> Client {
    let creds = Credentials::new(&cfg.access_key, &cfg.secret_key, None, None, "gig");
    let s3_config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .credentials_provider(creds)
        .region(Region::new(cfg.region.clone()))
        .endpoint_url(endpoint)
        .force_path_style(cfg.path_style)
        .build();
    Client::from_conf(s3_config)
}

impl S3Uploader {
    /// Simple single-request upload via PutObject.
    fn put_object(&self, local: &Path, key: &str) -> Result<()> {
        let content =
            std::fs::read(local).map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))?;

        self.rt
            .block_on(async {
                self.client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(key)
                    .body(ByteStream::from(content))
                    .content_type("application/octet-stream")
                    .send()
                    .await
            })
            .map_err(|e| Error::Invalid(format!("S3 PUT failed: {e}")))?;

        Ok(())
    }

    /// Best-effort abort of an in-progress multipart upload.
    fn abort_multipart(&self, key: &str, upload_id: &str) {
        let _ = self.rt.block_on(async {
            self.client
                .abort_multipart_upload()
                .bucket(&self.bucket)
                .key(key)
                .upload_id(upload_id)
                .send()
                .await
        });
    }

    /// Multipart upload for large files.
    fn multipart_upload(&self, local: &Path, key: &str) -> Result<()> {
        let mut file = std::fs::File::open(local)
            .map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))?;
        let file_size = file
            .metadata()
            .map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))?
            .len();

        // Initiate multipart upload.
        let upload_id = self
            .rt
            .block_on(async {
                self.client
                    .create_multipart_upload()
                    .bucket(&self.bucket)
                    .key(key)
                    .content_type("application/octet-stream")
                    .send()
                    .await
            })
            .map_err(|e| Error::Invalid(format!("S3 create multipart upload failed: {e}")))?
            .upload_id()
            .ok_or_else(|| Error::Invalid("S3 multipart upload returned no upload_id".into()))?
            .to_string();

        // Upload parts.
        let mut completed_parts: Vec<CompletedPart> = Vec::new();
        let mut remaining = file_size;
        let mut part_number: i32 = 1;

        while remaining > 0 {
            let chunk_size = std::cmp::min(PART_SIZE as u64, remaining) as usize;
            let mut buf = vec![0u8; chunk_size];
            if let Err(e) = file
                .read_exact(&mut buf)
                .map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))
            {
                self.abort_multipart(key, &upload_id);
                return Err(e);
            }

            let upload_part_output = match self.rt.block_on(async {
                self.client
                    .upload_part()
                    .bucket(&self.bucket)
                    .key(key)
                    .upload_id(&upload_id)
                    .part_number(part_number)
                    .body(ByteStream::from(buf))
                    .send()
                    .await
            }) {
                Ok(output) => output,
                Err(e) => {
                    self.abort_multipart(key, &upload_id);
                    return Err(Error::Invalid(format!(
                        "S3 upload part {part_number} failed: {e}"
                    )));
                }
            };

            completed_parts.push(
                CompletedPart::builder()
                    .e_tag(upload_part_output.e_tag().unwrap_or_default())
                    .part_number(part_number)
                    .build(),
            );

            remaining -= chunk_size as u64;
            part_number += 1;
        }

        // Complete multipart upload.
        let completed_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        if let Err(e) = self.rt.block_on(async {
            self.client
                .complete_multipart_upload()
                .bucket(&self.bucket)
                .key(key)
                .upload_id(&upload_id)
                .multipart_upload(completed_upload)
                .send()
                .await
        }) {
            self.abort_multipart(key, &upload_id);
            return Err(Error::Invalid(format!(
                "S3 complete multipart upload failed: {e}"
            )));
        }

        Ok(())
    }

    fn presigned_get_url(&self, object_key: &str, file_name: &str, ttl: u32) -> Result<String> {
        let disposition = format!("attachment; filename=\"{file_name}\"");
        let presigned = self.rt.block_on(async {
            let presign_config = PresigningConfig::expires_in(Duration::from_secs(ttl as u64))
                .map_err(|e| Error::Invalid(format!("presigning config error: {e}")))?;
            self.download_client
                .get_object()
                .bucket(&self.bucket)
                .key(object_key)
                .response_content_disposition(&disposition)
                .presigned(presign_config)
                .await
                .map_err(|e| Error::Invalid(format!("presigning failed: {e}")))
        })?;
        Ok(presigned.uri().to_string())
    }
}

impl Uploader for S3Uploader {
    fn name(&self) -> &str {
        &self.name
    }

    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
        let file_name = local
            .file_name()
            .ok_or_else(|| Error::Invalid("local path has no filename".into()))?
            .to_string_lossy()
            .into_owned();
        let object_key = opts.object_key.as_deref().unwrap_or(&file_name);

        let file_size = std::fs::metadata(local)
            .map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))?
            .len();

        if file_size >= MULTIPART_THRESHOLD {
            self.multipart_upload(local, object_key)?;
        } else {
            self.put_object(local, object_key)?;
        }

        // Generate presigned GET URL with Content-Disposition for proper filename.
        let ttl = opts
            .link_ttl_days
            .map(|d| d * 86_400)
            .unwrap_or(self.link_ttl_seconds);

        let url = self.presigned_get_url(object_key, &file_name, ttl)?;
        let now = OffsetDateTime::now_utc();
        let expires_at = now.unix_timestamp() + ttl as i64;

        Ok(UploadResult {
            url,
            expires_at: Some(expires_at),
            provider: self.name.clone(),
            file_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::S3UploaderConfig;

    fn make_config() -> S3UploaderConfig {
        S3UploaderConfig {
            bucket: "gig-delivery".into(),
            region: "cn-hongkong".into(),
            endpoint: "https://s3.oss-cn-hongkong.aliyuncs.com".into(),
            download_endpoint: None,
            access_key: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            link_ttl_seconds: 604_800,
            path_style: false,
        }
    }

    #[test]
    fn s3_uploader_construction() {
        let cfg = make_config();
        let u = S3Uploader::new("s3:test".into(), &cfg).unwrap();
        assert_eq!(u.name(), "s3:test");
        assert_eq!(u.bucket, "gig-delivery");
        assert_eq!(u.link_ttl_seconds, 604_800);
    }

    #[test]
    fn presigned_get_url_uses_download_endpoint_when_configured() {
        let mut cfg = make_config();
        cfg.download_endpoint = Some("https://oss-accelerate.aliyuncs.com".into());
        let u = S3Uploader::new("s3:test".into(), &cfg).unwrap();

        let url = u
            .presigned_get_url("orders/1/client-package.zip", "client-package.zip", 3600)
            .unwrap();

        assert!(
            url.starts_with("https://gig-delivery.oss-accelerate.aliyuncs.com/"),
            "expected accelerated download host, got {url}"
        );
    }
}
