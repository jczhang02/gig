//! Native S3-compatible uploader. Works with AWS S3, Alibaba Cloud OSS,
//! Cloudflare R2, MinIO, and any S3-compatible service.
//!
//! Generates presigned download URLs so buckets can stay private.
//!
//! Upload uses a presigned PUT URL (SigV4) for the file bytes, then
//! generates a presigned GET URL for the shareable download link.

use super::{UploadOpts, UploadResult, Uploader};
use crate::{Error, Result};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::path::Path;
use time::OffsetDateTime;

type HmacSha256 = Hmac<Sha256>;

/// Uploader that talks directly to any S3-compatible API using AWS Signature V4.
///
/// Works with:
/// - Alibaba Cloud OSS (`endpoint = "https://oss-cn-hongkong.aliyuncs.com"`)
/// - AWS S3 (`endpoint = "https://s3.amazonaws.com"`)
/// - Cloudflare R2 (`endpoint = "https://<account>.r2.cloudflarestorage.com"`)
/// - MinIO (set `path_style = true`)
pub struct S3Uploader {
    /// Human-readable name used in config and artifact records (e.g. `"s3:aliyun-hk"`).
    pub name: String,
    /// Bucket name (e.g. `"gig-delivery"`).
    pub bucket: String,
    /// Region identifier (e.g. `"oss-cn-hongkong"` or `"us-east-1"`).
    pub region: String,
    /// Base endpoint URL without trailing slash
    /// (e.g. `"https://oss-cn-hongkong.aliyuncs.com"`).
    pub endpoint: String,
    /// AWS / OSS access key ID.
    pub access_key: String,
    /// AWS / OSS secret access key.
    pub secret_key: String,
    /// How long presigned GET links remain valid (seconds). Default: 604800 (7 days).
    pub link_ttl_seconds: u32,
    /// Use path-style URLs (`endpoint/bucket/key`) instead of virtual-hosted
    /// (`bucket.endpoint/key`). Required for MinIO; not needed for OSS/AWS.
    pub path_style: bool,
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

        let content =
            std::fs::read(local).map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))?;

        // --- Upload via presigned PUT ---
        let now = OffsetDateTime::now_utc();
        let put_url = self.presign_put(&file_name, &content, now)?;

        let client = reqwest::blocking::Client::new();
        let resp = client
            .put(&put_url)
            .header("Content-Type", "application/octet-stream")
            .body(content)
            .send()
            .map_err(|e| Error::Invalid(format!("S3 PUT request failed: {e}")))?;

        let status = resp.status().as_u16();
        if status != 200 {
            let body = resp.text().unwrap_or_default();
            return Err(Error::Invalid(format!(
                "S3 PUT returned status {status}: {body}"
            )));
        }

        // --- Generate presigned GET URL for the shareable link ---
        let ttl = opts
            .link_ttl_days
            .map(|d| d * 86_400)
            .unwrap_or(self.link_ttl_seconds);

        let get_now = OffsetDateTime::now_utc();
        let presigned_url = self.presign_get(&file_name, ttl, get_now)?;

        let expires_at = get_now.unix_timestamp() + ttl as i64;

        Ok(UploadResult {
            url: presigned_url,
            expires_at: Some(expires_at),
            provider: self.name.clone(),
        })
    }
}

impl S3Uploader {
    /// Build the host portion of the URL.
    ///
    /// - Path-style (MinIO):   `endpoint/bucket`
    /// - Virtual-hosted (OSS): `bucket.host` derived from `endpoint`
    fn object_url(&self, key: &str) -> String {
        if self.path_style {
            // endpoint already includes scheme: "https://minio.example.com"
            format!(
                "{}/{}/{}",
                self.endpoint.trim_end_matches('/'),
                self.bucket,
                key
            )
        } else {
            // Insert bucket as subdomain: "https://oss-cn-hongkong.aliyuncs.com"
            // → "https://gig-delivery.oss-cn-hongkong.aliyuncs.com"
            let endpoint = self.endpoint.trim_end_matches('/');
            if let Some(rest) = endpoint.strip_prefix("https://") {
                format!("https://{}.{}/{}", self.bucket, rest, key)
            } else if let Some(rest) = endpoint.strip_prefix("http://") {
                format!("http://{}.{}/{}", self.bucket, rest, key)
            } else {
                // Fallback: path-style
                format!("{}/{}/{}", endpoint, self.bucket, key)
            }
        }
    }

    /// Derive the hostname used as S3 `Host` header (and canonical host in SigV4).
    fn host(&self) -> String {
        let endpoint = self.endpoint.trim_end_matches('/');
        let bare = endpoint
            .strip_prefix("https://")
            .or_else(|| endpoint.strip_prefix("http://"))
            .unwrap_or(endpoint);

        if self.path_style {
            bare.to_string()
        } else {
            format!("{}.{}", self.bucket, bare)
        }
    }

    /// Generate a presigned PUT URL for uploading `content`.
    fn presign_put(&self, key: &str, content: &[u8], now: OffsetDateTime) -> Result<String> {
        let content_sha256 = hex_sha256(content);
        self.presign(
            "PUT",
            key,
            &[("x-amz-content-sha256", content_sha256.as_str())],
            3600, // 1 hour is plenty for the upload window
            now,
        )
    }

    /// Generate a presigned GET URL valid for `ttl_seconds`.
    fn presign_get(&self, key: &str, ttl_seconds: u32, now: OffsetDateTime) -> Result<String> {
        self.presign("GET", key, &[], ttl_seconds, now)
    }

    /// Core AWS Signature V4 presigned URL generation.
    ///
    /// Reference: <https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-query-string-auth.html>
    fn presign(
        &self,
        method: &str,
        key: &str,
        extra_signed_headers: &[(&str, &str)],
        expires: u32,
        now: OffsetDateTime,
    ) -> Result<String> {
        // Timestamps
        let date_str = format!("{:04}{:02}{:02}", now.year(), now.month() as u8, now.day());
        let datetime_str = format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            now.year(),
            now.month() as u8,
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        );

        let host = self.host();
        let credential_scope = format!("{}/{}/s3/aws4_request", date_str, self.region);
        let credential = format!("{}/{}", self.access_key, credential_scope);

        // Signed headers: always include "host"; add extras for PUT
        let mut signed_header_names: Vec<String> = vec!["host".to_string()];
        for (name, _) in extra_signed_headers {
            signed_header_names.push(name.to_lowercase());
        }
        signed_header_names.sort();
        let signed_headers_str = signed_header_names.join(";");

        // Build canonical query string (sorted by parameter name)
        let mut query_params: Vec<(String, String)> = vec![
            ("X-Amz-Algorithm".into(), "AWS4-HMAC-SHA256".into()),
            ("X-Amz-Credential".into(), urlencode(&credential)),
            ("X-Amz-Date".into(), datetime_str.clone()),
            ("X-Amz-Expires".into(), expires.to_string()),
            ("X-Amz-SignedHeaders".into(), urlencode(&signed_headers_str)),
        ];
        // Sort alphabetically by key (required for canonical form)
        query_params.sort_by(|a, b| a.0.cmp(&b.0));
        let canonical_query: String = query_params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencode(k), v))
            .collect::<Vec<_>>()
            .join("&");

        // Canonical headers
        let mut canonical_headers_lines: Vec<String> = vec![format!("host:{}", host)];
        for (name, value) in extra_signed_headers {
            canonical_headers_lines.push(format!("{}:{}", name.to_lowercase(), value.trim()));
        }
        canonical_headers_lines.sort();
        let canonical_headers = canonical_headers_lines.join("\n") + "\n";

        // Canonical URI: percent-encode the key (but preserve '/')
        let canonical_uri = if self.path_style {
            format!("/{}/{}", self.bucket, urlencode_path(key))
        } else {
            format!("/{}", urlencode_path(key))
        };

        // For presigned URLs, payload hash is always UNSIGNED-PAYLOAD or the actual hash
        let payload_hash = if extra_signed_headers
            .iter()
            .any(|(k, _)| *k == "x-amz-content-sha256")
        {
            extra_signed_headers
                .iter()
                .find(|(k, _)| *k == "x-amz-content-sha256")
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| "UNSIGNED-PAYLOAD".into())
        } else {
            "UNSIGNED-PAYLOAD".into()
        };

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method,
            canonical_uri,
            canonical_query,
            canonical_headers,
            signed_headers_str,
            payload_hash
        );

        // String to sign
        let cr_hash = hex_sha256(canonical_request.as_bytes());
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            datetime_str, credential_scope, cr_hash
        );

        // Signing key
        let signing_key = derive_signing_key(&self.secret_key, &date_str, &self.region, "s3")?;
        let signature = hex_hmac_sha256(&signing_key, string_to_sign.as_bytes())?;

        // Final presigned URL
        let base_url = self.object_url(key);
        let presigned = format!(
            "{}?{}&X-Amz-Signature={}",
            base_url, canonical_query, signature
        );

        Ok(presigned)
    }
}

// ── Crypto helpers ────────────────────────────────────────────────────────────

fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| Error::Invalid(format!("HMAC key error: {e}")))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn hex_hmac_sha256(key: &[u8], data: &[u8]) -> Result<String> {
    Ok(hex::encode(hmac_sha256(key, data)?))
}

/// Derive the AWS SigV4 signing key:
/// HMAC-SHA256(HMAC-SHA256(HMAC-SHA256(HMAC-SHA256("AWS4" + secret, date), region), service), "aws4_request")
fn derive_signing_key(secret: &str, date: &str, region: &str, service: &str) -> Result<Vec<u8>> {
    let k_secret = format!("AWS4{}", secret);
    let k_date = hmac_sha256(k_secret.as_bytes(), date.as_bytes())?;
    let k_region = hmac_sha256(&k_date, region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, service.as_bytes())?;
    hmac_sha256(&k_service, b"aws4_request")
}

// ── URL encoding helpers ──────────────────────────────────────────────────────

/// Percent-encode a string value for use in query parameters.
/// Encodes everything except unreserved chars: A-Z a-z 0-9 - _ . ~
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

/// Percent-encode a path segment, preserving '/' separators.
fn urlencode_path(s: &str) -> String {
    s.split('/').map(urlencode).collect::<Vec<_>>().join("/")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_uploader(path_style: bool) -> S3Uploader {
        S3Uploader {
            name: "s3:test".into(),
            bucket: "gig-delivery".into(),
            region: "oss-cn-hongkong".into(),
            endpoint: "https://oss-cn-hongkong.aliyuncs.com".into(),
            access_key: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            link_ttl_seconds: 604_800,
            path_style,
        }
    }

    #[test]
    fn s3_uploader_construction() {
        let u = make_uploader(false);
        assert_eq!(u.name(), "s3:test");
        assert_eq!(u.bucket, "gig-delivery");
        assert_eq!(u.link_ttl_seconds, 604_800);
    }

    #[test]
    fn virtual_hosted_object_url() {
        let u = make_uploader(false);
        let url = u.object_url("myfile.zip");
        assert_eq!(
            url,
            "https://gig-delivery.oss-cn-hongkong.aliyuncs.com/myfile.zip"
        );
    }

    #[test]
    fn path_style_object_url() {
        let mut u = make_uploader(true);
        u.endpoint = "https://minio.example.com".into();
        let url = u.object_url("myfile.zip");
        assert_eq!(url, "https://minio.example.com/gig-delivery/myfile.zip");
    }

    #[test]
    fn presign_get_produces_valid_url_structure() {
        let u = make_uploader(false);
        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let url = u.presign_get("test.zip", 3600, now).unwrap();

        // Must be HTTPS and contain required SigV4 query params
        assert!(url.starts_with("https://"), "url: {url}");
        assert!(
            url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"),
            "url: {url}"
        );
        assert!(url.contains("X-Amz-Credential="), "url: {url}");
        assert!(url.contains("X-Amz-Date="), "url: {url}");
        assert!(url.contains("X-Amz-Expires=3600"), "url: {url}");
        assert!(url.contains("X-Amz-Signature="), "url: {url}");
        assert!(url.contains("test.zip"), "url: {url}");
    }

    #[test]
    fn urlencode_encodes_special_chars() {
        assert_eq!(urlencode("hello world"), "hello%20world");
        assert_eq!(urlencode("a/b"), "a%2Fb");
        assert_eq!(urlencode("abc-_.~"), "abc-_.~");
    }

    #[test]
    fn urlencode_path_preserves_slashes() {
        assert_eq!(urlencode_path("dir/file name.zip"), "dir/file%20name.zip");
    }

    #[test]
    fn hex_sha256_empty() {
        // SHA256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let h = hex_sha256(b"");
        assert_eq!(
            h,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn credential_scope_format() {
        let u = make_uploader(false);
        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        // 2023-11-14T22:13:20Z → date = 20231114
        let url = u.presign_get("f.zip", 600, now).unwrap();
        assert!(url.contains("20231114"), "expected date in url: {url}");
        assert!(
            url.contains("oss-cn-hongkong%2Fs3%2Faws4_request"),
            "url: {url}"
        );
    }
}
