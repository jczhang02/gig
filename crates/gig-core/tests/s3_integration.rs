//! Integration tests for S3-compatible upload (Alibaba Cloud OSS).
//!
//! These tests require real credentials and are skipped by default.
//! Run with: `cargo test -p gig-core -- --ignored`
//!
//! Required env vars:
//!   GIG_TEST_S3_BUCKET, GIG_TEST_S3_ACCESS_KEY, GIG_TEST_S3_SECRET_KEY
//! Optional:
//!   GIG_TEST_S3_REGION (default: cn-hongkong)
//!   GIG_TEST_S3_ENDPOINT (default: https://s3.oss-cn-hongkong.aliyuncs.com)

use gig_core::config::S3UploaderConfig;
use gig_core::delivery::s3::S3Uploader;
use gig_core::delivery::{UploadOpts, Uploader};
use std::io::Write;
use tempfile::NamedTempFile;

// Note: presigned URLs expire naturally based on link_ttl_seconds.
// Uploaded objects accumulate in the bucket and should be cleaned up separately
// (e.g. via a lifecycle rule or manual deletion after testing).

fn make_test_uploader() -> Option<S3Uploader> {
    let bucket = std::env::var("GIG_TEST_S3_BUCKET").ok()?;
    let access_key = std::env::var("GIG_TEST_S3_ACCESS_KEY").ok()?;
    let secret_key = std::env::var("GIG_TEST_S3_SECRET_KEY").ok()?;
    let region = std::env::var("GIG_TEST_S3_REGION")
        .unwrap_or_else(|_| "cn-hongkong".into());
    let endpoint = std::env::var("GIG_TEST_S3_ENDPOINT")
        .unwrap_or_else(|_| "https://s3.oss-cn-hongkong.aliyuncs.com".into());

    let cfg = S3UploaderConfig {
        bucket,
        region,
        endpoint,
        access_key,
        secret_key,
        link_ttl_seconds: 3600,
        path_style: false,
    };

    S3Uploader::new("s3:test".into(), &cfg).ok()
}

fn default_opts() -> UploadOpts {
    UploadOpts { link_ttl_days: None }
}

#[test]
#[ignore]
fn test_upload_small_file() {
    let uploader = match make_test_uploader() {
        Some(u) => u,
        None => {
            eprintln!("Skipping test_upload_small_file: required env vars not set");
            return;
        }
    };

    eprintln!("test_upload_small_file: creating 1KB temp file");
    let mut tmp = NamedTempFile::new().expect("failed to create temp file");
    let content = vec![0xABu8; 1024];
    tmp.write_all(&content).expect("failed to write temp file");

    eprintln!("test_upload_small_file: uploading");
    let result = uploader.upload(tmp.path(), &default_opts()).expect("upload failed");

    eprintln!("test_upload_small_file: url={}", result.url);
    assert!(!result.url.is_empty(), "URL should be non-empty");
    assert!(
        result.provider.starts_with("s3:"),
        "provider should start with 's3:', got: {}",
        result.provider
    );
    assert!(result.expires_at.is_some(), "expires_at should be Some");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(
        result.expires_at.unwrap() > now,
        "expires_at should be in the future"
    );

    eprintln!("test_upload_small_file: PASSED");
}

#[test]
#[ignore]
fn test_upload_medium_file() {
    let uploader = match make_test_uploader() {
        Some(u) => u,
        None => {
            eprintln!("Skipping test_upload_medium_file: required env vars not set");
            return;
        }
    };

    eprintln!("test_upload_medium_file: creating 5MB temp file");
    let mut tmp = NamedTempFile::new().expect("failed to create temp file");
    let content = vec![0xABu8; 5 * 1024 * 1024];
    tmp.write_all(&content).expect("failed to write temp file");

    eprintln!("test_upload_medium_file: uploading");
    let result = uploader.upload(tmp.path(), &default_opts()).expect("upload failed");

    eprintln!("test_upload_medium_file: url={}", result.url);
    assert!(!result.url.is_empty(), "URL should be non-empty");
    assert!(
        result.provider.starts_with("s3:"),
        "provider should start with 's3:', got: {}",
        result.provider
    );
    assert!(result.expires_at.is_some(), "expires_at should be Some");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(
        result.expires_at.unwrap() > now,
        "expires_at should be in the future"
    );

    eprintln!("test_upload_medium_file: PASSED");
}

#[test]
#[ignore]
fn test_upload_large_file_multipart() {
    let uploader = match make_test_uploader() {
        Some(u) => u,
        None => {
            eprintln!("Skipping test_upload_large_file_multipart: required env vars not set");
            return;
        }
    };

    // 120 MB exceeds the 100 MB multipart threshold
    let total_bytes: usize = 120 * 1024 * 1024;
    eprintln!(
        "test_upload_large_file_multipart: creating {}MB temp file",
        total_bytes / (1024 * 1024)
    );

    let mut tmp = NamedTempFile::new().expect("failed to create temp file");
    // Write in 8MB chunks to avoid a large stack allocation
    let chunk = vec![0xABu8; 8 * 1024 * 1024];
    let mut written = 0;
    while written < total_bytes {
        let remaining = total_bytes - written;
        let to_write = remaining.min(chunk.len());
        tmp.write_all(&chunk[..to_write]).expect("failed to write chunk");
        written += to_write;
    }

    eprintln!("test_upload_large_file_multipart: uploading (multipart path expected)");
    let result = uploader.upload(tmp.path(), &default_opts()).expect("upload failed");

    eprintln!("test_upload_large_file_multipart: url={}", result.url);
    assert!(!result.url.is_empty(), "URL should be non-empty");
    assert!(
        result.provider.starts_with("s3:"),
        "provider should start with 's3:', got: {}",
        result.provider
    );
    assert!(result.expires_at.is_some(), "expires_at should be Some");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(
        result.expires_at.unwrap() > now,
        "expires_at should be in the future"
    );

    eprintln!("test_upload_large_file_multipart: PASSED");
}

#[test]
#[ignore]
fn test_upload_multiple_files() {
    let uploader = match make_test_uploader() {
        Some(u) => u,
        None => {
            eprintln!("Skipping test_upload_multiple_files: required env vars not set");
            return;
        }
    };

    eprintln!("test_upload_multiple_files: uploading 3 small files");
    let mut urls = Vec::new();

    for i in 0..3usize {
        let mut tmp = NamedTempFile::new().expect("failed to create temp file");
        // Use distinct content so files are meaningfully different
        let content = vec![(0xAB + i as u8) % 0xFF; 1024];
        tmp.write_all(&content).expect("failed to write temp file");

        let result = uploader
            .upload(tmp.path(), &default_opts())
            .unwrap_or_else(|e| panic!("upload {} failed: {}", i, e));

        eprintln!("test_upload_multiple_files: file {} url={}", i, result.url);
        assert!(!result.url.is_empty(), "URL {} should be non-empty", i);
        urls.push(result.url);
    }

    // Each upload should produce a unique presigned URL
    let unique: std::collections::HashSet<&String> = urls.iter().collect();
    assert_eq!(unique.len(), urls.len(), "all URLs should be unique");

    eprintln!("test_upload_multiple_files: PASSED");
}

#[test]
#[ignore]
fn test_presigned_url_accessible() {
    let uploader = match make_test_uploader() {
        Some(u) => u,
        None => {
            eprintln!("Skipping test_presigned_url_accessible: required env vars not set");
            return;
        }
    };

    eprintln!("test_presigned_url_accessible: uploading small file");
    let mut tmp = NamedTempFile::new().expect("failed to create temp file");
    tmp.write_all(&vec![0xABu8; 512]).expect("failed to write temp file");

    let result = uploader.upload(tmp.path(), &default_opts()).expect("upload failed");
    eprintln!("test_presigned_url_accessible: url={}", result.url);

    eprintln!("test_presigned_url_accessible: checking HTTP status via curl");
    let output = std::process::Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", &result.url])
        .output()
        .expect("failed to run curl");

    let status_str = String::from_utf8_lossy(&output.stdout);
    let http_status: u32 = status_str
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("unexpected curl output: {}", status_str));

    eprintln!("test_presigned_url_accessible: HTTP status={}", http_status);
    assert_eq!(http_status, 200, "presigned URL should return HTTP 200");

    eprintln!("test_presigned_url_accessible: PASSED");
}
