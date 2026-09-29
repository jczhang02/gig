//! S3-compatible uploader (AWS S3, Alibaba OSS, R2, MinIO) via the AWS SDK.
//! Ported from v1: PutObject below 100 MB, multipart above, presigned GET with
//! a Content-Disposition attachment header so the download keeps its filename.

use super::{validate_https_or_allowed_http_url, UploadOpts, UploadResult, Uploader};
use crate::secrets::ResolvedS3;
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

const MULTIPART_THRESHOLD: u64 = 100 * 1024 * 1024;
const PART_SIZE: usize = 8 * 1024 * 1024;

pub struct S3Uploader {
    name: String,
    client: Client,
    download_client: Client,
    rt: tokio::runtime::Runtime,
    bucket: String,
    link_ttl_seconds: u32,
}

impl S3Uploader {
    pub fn new(resolved: &ResolvedS3) -> Result<Self> {
        let cfg = &resolved.config;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::Upload(format!("failed to create tokio runtime: {e}")))?;
        if cfg.bucket.trim().is_empty() || cfg.region.trim().is_empty() {
            return Err(Error::Config(format!(
                "uploader {} needs bucket and region",
                resolved.name
            )));
        }
        validate_https_or_allowed_http_url(&cfg.endpoint, "S3 endpoint", cfg.allow_insecure_http)?;
        if let Some(d) = cfg
            .download_endpoint
            .as_deref()
            .filter(|d| !d.trim().is_empty())
        {
            validate_https_or_allowed_http_url(d, "S3 download_endpoint", cfg.allow_insecure_http)?;
        }
        let client = make_client(resolved, &cfg.endpoint);
        let download_endpoint = cfg
            .download_endpoint
            .as_deref()
            .filter(|d| !d.trim().is_empty())
            .unwrap_or(&cfg.endpoint);
        let download_client = make_client(resolved, download_endpoint);
        Ok(Self {
            name: resolved.name.clone(),
            client,
            download_client,
            rt,
            bucket: cfg.bucket.clone(),
            link_ttl_seconds: resolved.link_ttl_seconds,
        })
    }

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
            .map_err(|e| Error::Upload(format!("S3 PUT failed: {e}")))?;
        Ok(())
    }

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

    fn multipart_upload(&self, local: &Path, key: &str, opts: &UploadOpts) -> Result<()> {
        let mut file = std::fs::File::open(local)
            .map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))?;
        let file_size = file
            .metadata()
            .map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))?
            .len();
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
            .map_err(|e| Error::Upload(format!("S3 create multipart upload failed: {e}")))?
            .upload_id()
            .ok_or_else(|| Error::Upload("S3 multipart upload returned no upload_id".into()))?
            .to_string();

        let mut parts: Vec<CompletedPart> = Vec::new();
        let mut remaining = file_size;
        let mut part_number: i32 = 1;
        while remaining > 0 {
            let chunk = std::cmp::min(PART_SIZE as u64, remaining) as usize;
            let mut buf = vec![0u8; chunk];
            if let Err(e) = file.read_exact(&mut buf) {
                self.abort_multipart(key, &upload_id);
                return Err(Error::PathUnavailable(local.to_path_buf(), e));
            }
            let out = match self.rt.block_on(async {
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
                Ok(o) => o,
                Err(e) => {
                    self.abort_multipart(key, &upload_id);
                    return Err(Error::Upload(format!(
                        "S3 upload part {part_number} failed: {e}"
                    )));
                }
            };
            parts.push(
                CompletedPart::builder()
                    .e_tag(out.e_tag().unwrap_or_default())
                    .part_number(part_number)
                    .build(),
            );
            remaining -= chunk as u64;
            part_number += 1;
            opts.report(file_size - remaining, file_size);
        }
        let completed = CompletedMultipartUpload::builder()
            .set_parts(Some(parts))
            .build();
        if let Err(e) = self.rt.block_on(async {
            self.client
                .complete_multipart_upload()
                .bucket(&self.bucket)
                .key(key)
                .upload_id(&upload_id)
                .multipart_upload(completed)
                .send()
                .await
        }) {
            self.abort_multipart(key, &upload_id);
            return Err(Error::Upload(format!(
                "S3 complete multipart upload failed: {e}"
            )));
        }
        Ok(())
    }

    fn presigned_get_url(&self, object_key: &str, file_name: &str, ttl: u32) -> Result<String> {
        let disposition = format!("attachment; filename=\"{file_name}\"");
        let presigned = self.rt.block_on(async {
            let cfg = PresigningConfig::expires_in(Duration::from_secs(ttl as u64))
                .map_err(|e| Error::Upload(format!("presigning config error: {e}")))?;
            self.download_client
                .get_object()
                .bucket(&self.bucket)
                .key(object_key)
                .response_content_disposition(&disposition)
                .presigned(cfg)
                .await
                .map_err(|e| Error::Upload(format!("presigning failed: {e}")))
        })?;
        Ok(presigned.uri().to_string())
    }
}

fn make_client(resolved: &ResolvedS3, endpoint: &str) -> Client {
    let creds = Credentials::new(
        &resolved.access_key,
        &resolved.secret_key,
        None,
        None,
        "gig",
    );
    let cfg = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .credentials_provider(creds)
        .region(Region::new(resolved.config.region.clone()))
        .endpoint_url(endpoint)
        .force_path_style(resolved.config.path_style)
        .build();
    Client::from_conf(cfg)
}

impl Uploader for S3Uploader {
    fn name(&self) -> &str {
        &self.name
    }

    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
        let file_name = local
            .file_name()
            .ok_or_else(|| Error::InvalidInput("local path has no filename".into()))?
            .to_string_lossy()
            .into_owned();
        let object_key = opts.object_key.as_deref().unwrap_or(&file_name);
        let file_size = std::fs::metadata(local)
            .map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))?
            .len();
        if file_size >= MULTIPART_THRESHOLD {
            self.multipart_upload(local, object_key, opts)?;
        } else {
            self.put_object(local, object_key)?;
            opts.report(file_size, file_size);
        }
        let ttl = self.link_ttl_seconds;
        let url = self.presigned_get_url(object_key, &file_name, ttl)?;
        let expires_at = OffsetDateTime::now_utc().unix_timestamp() + ttl as i64;
        Ok(UploadResult {
            url,
            short_url: None,
            expires_at: Some(expires_at),
            provider: self.name.clone(),
            file_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::S3;

    fn resolved() -> ResolvedS3 {
        ResolvedS3 {
            name: "s3:test".into(),
            config: S3 {
                bucket: "gig-delivery".into(),
                region: "ap-southeast-1".into(),
                endpoint: "https://s3.example.test".into(),
                download_endpoint: None,
                path_style: false,
                allow_insecure_http: false,
            },
            access_key: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            link_ttl_seconds: 604_800,
        }
    }

    #[test]
    fn constructs_and_validates_endpoints() {
        let u = S3Uploader::new(&resolved()).unwrap();
        assert_eq!(u.name(), "s3:test");
        let mut r = resolved();
        r.config.endpoint = "http://oss.example.test".into();
        assert!(S3Uploader::new(&r).is_err());
        r.config.allow_insecure_http = true;
        assert!(S3Uploader::new(&r).is_ok());
        let mut r = resolved();
        r.config.endpoint = "http://127.0.0.1:9000".into();
        assert!(S3Uploader::new(&r).is_ok());
    }

    #[test]
    fn presigned_url_uses_download_endpoint_and_disposition() {
        let mut r = resolved();
        r.config.download_endpoint = Some("https://dl.example.test".into());
        let u = S3Uploader::new(&r).unwrap();
        let url = u
            .presigned_get_url("o/p-v1/p-v1.zip", "p-v1.zip", 3600)
            .unwrap();
        assert!(
            url.starts_with("https://gig-delivery.dl.example.test/"),
            "{url}"
        );
        assert!(url.contains("response-content-disposition="));
    }
}
