//! Delivery trait and uploader backends.
//!
//! The `Uploader` trait is used by `gig package send` and `gig artifact send`.
//! Backends live in submodules.

pub mod s3;

pub use s3::S3Uploader;

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;
use time::OffsetDateTime;

/// Options passed to every uploader.
#[derive(Debug, Clone)]
pub struct UploadOpts {
    /// If set, request that the shared link expires after this many days.
    /// Not all backends honour this; it is advisory.
    pub link_ttl_days: Option<u32>,
    /// If set, use this backend object key instead of the local basename.
    pub object_key: Option<String>,
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
    /// Size of the uploaded file in bytes.
    pub file_size: u64,
}

/// The pluggable uploader interface.
pub trait Uploader: Send + Sync {
    /// Short, stable name used in config and artifact records (e.g. "rclone").
    fn name(&self) -> &str;

    /// Upload `local` file to the backend and return a shareable link.
    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult>;
}

pub trait ShortLinker: Send + Sync {
    fn shorten(&self, long_url: &str, ttl_seconds: Option<u32>) -> Result<String>;
}

pub struct ShorteningUploader {
    inner: Box<dyn Uploader>,
    short_linker: Box<dyn ShortLinker>,
}

#[derive(Clone)]
pub struct HttpShortLinker {
    endpoint: String,
    token: String,
    agent: ureq::Agent,
}

#[derive(Debug, Serialize)]
struct ShortLinkCreateRequest<'a> {
    url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ShortLinkCreateResponse {
    short_url: String,
}

impl ShorteningUploader {
    pub fn new(inner: Box<dyn Uploader>, short_linker: Box<dyn ShortLinker>) -> Self {
        Self {
            inner,
            short_linker,
        }
    }
}

impl HttpShortLinker {
    pub fn new(endpoint: String, token: String) -> Result<Self> {
        if endpoint.trim().is_empty() {
            return Err(Error::Config("short link endpoint is empty".into()));
        }
        if token.trim().is_empty() {
            return Err(Error::Config("short link token is empty".into()));
        }
        validate_https_or_loopback_url(&endpoint, "short link endpoint")?;
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(30))
            .build();
        Ok(Self {
            endpoint,
            token,
            agent,
        })
    }
}

impl ShortLinker for HttpShortLinker {
    fn shorten(&self, long_url: &str, ttl_seconds: Option<u32>) -> Result<String> {
        let auth = format!("Bearer {}", self.token);
        let response: ShortLinkCreateResponse = self
            .agent
            .post(&self.endpoint)
            .set("Authorization", &auth)
            .send_json(ShortLinkCreateRequest {
                url: long_url,
                ttl_seconds,
            })
            .map_err(|err| Error::Invalid(format!("short link request failed: {err}")))?
            .into_json()
            .map_err(|err| Error::Invalid(format!("short link response invalid: {err}")))?;

        if response.short_url.trim().is_empty() {
            return Err(Error::Invalid(
                "short link response missing short_url".into(),
            ));
        }
        validate_https_or_loopback_url(&response.short_url, "short link response short_url")?;
        Ok(response.short_url)
    }
}

pub(crate) fn validate_https_or_loopback_url(value: &str, label: &str) -> Result<()> {
    validate_https_or_allowed_http_url(value, label, false)
}

pub(crate) fn validate_https_or_allowed_http_url(
    value: &str,
    label: &str,
    allow_insecure_http: bool,
) -> Result<()> {
    let parsed = ParsedUrl::parse(value, label)?;
    if parsed.scheme.eq_ignore_ascii_case("https") {
        return Ok(());
    }
    if parsed.scheme.eq_ignore_ascii_case("http")
        && (allow_insecure_http || is_loopback_authority(parsed.authority))
    {
        return Ok(());
    }
    Err(Error::Config(format!(
        "{label} must use https:// unless it targets loopback localhost or insecure HTTP is explicitly enabled"
    )))
}

struct ParsedUrl<'a> {
    scheme: &'a str,
    authority: &'a str,
}

impl<'a> ParsedUrl<'a> {
    fn parse(value: &'a str, label: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty()
            || value
                .chars()
                .any(|ch| ch.is_control() || ch.is_whitespace())
        {
            return Err(Error::Config(format!("{label} must be a valid URL")));
        }
        let Some((scheme, rest)) = value.split_once("://") else {
            return Err(Error::Config(format!("{label} must include a URL scheme")));
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if scheme.is_empty() || authority.is_empty() || host_from_authority(authority).is_empty() {
            return Err(Error::Config(format!("{label} must include a URL host")));
        }
        Ok(Self { scheme, authority })
    }
}

fn is_loopback_authority(authority: &str) -> bool {
    let host = host_from_authority(authority);
    host.eq_ignore_ascii_case("localhost")
        || match host.parse::<IpAddr>() {
            Ok(ip) => ip.is_loopback(),
            Err(_) => false,
        }
}

fn host_from_authority(authority: &str) -> &str {
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(rest) = host_port.strip_prefix('[') {
        return rest.split(']').next().unwrap_or_default();
    }
    host_port.split(':').next().unwrap_or_default()
}

impl Uploader for ShorteningUploader {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
        let mut result = self.inner.upload(local, opts)?;
        let ttl_seconds = ttl_from_expiry(result.expires_at);
        result.url = self.short_linker.shorten(&result.url, ttl_seconds)?;
        Ok(result)
    }
}

fn ttl_from_expiry(expires_at: Option<i64>) -> Option<u32> {
    let expires_at = expires_at?;
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let ttl = expires_at.checked_sub(now)?;
    u32::try_from(ttl).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::thread;

    #[derive(Debug)]
    struct StaticUploader;

    impl Uploader for StaticUploader {
        fn name(&self) -> &str {
            "test:s3"
        }

        fn upload(&self, _local: &Path, _opts: &UploadOpts) -> Result<UploadResult> {
            Ok(UploadResult {
                url: "https://s3.example.test/long?signature=abc".to_string(),
                expires_at: Some(1_780_000_000),
                provider: self.name().to_string(),
                file_size: 123,
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingShortLinker {
        calls: Mutex<Vec<String>>,
    }

    impl ShortLinker for RecordingShortLinker {
        fn shorten(&self, long_url: &str, _ttl_seconds: Option<u32>) -> Result<String> {
            self.calls.lock().unwrap().push(long_url.to_string());
            Ok("https://go.jczhang.cc/a1b2c3d4".to_string())
        }
    }

    #[test]
    fn shortening_uploader_replaces_backend_url_and_preserves_metadata() {
        let shortener = RecordingShortLinker::default();
        let uploader = ShorteningUploader::new(Box::new(StaticUploader), Box::new(shortener));
        let local = PathBuf::from("client-package.zip");

        let result = uploader
            .upload(
                &local,
                &UploadOpts {
                    link_ttl_days: None,
                    object_key: None,
                },
            )
            .unwrap();

        assert_eq!(result.url, "https://go.jczhang.cc/a1b2c3d4");
        assert_eq!(result.expires_at, Some(1_780_000_000));
        assert_eq!(result.provider, "test:s3");
        assert_eq!(result.file_size, 123);
    }

    #[test]
    fn http_short_linker_rejects_plain_http_non_loopback_endpoint() {
        let err = match HttpShortLinker::new("http://example.com/api/links".into(), "token".into())
        {
            Ok(_) => panic!("plain HTTP endpoint should be rejected"),
            Err(err) => err.to_string(),
        };

        assert!(err.contains("https://"));
    }

    #[test]
    fn http_short_linker_rejects_invalid_endpoint_url() {
        for endpoint in [
            "https://",
            "https:// bad.example",
            "go.example.test/api/links",
        ] {
            let err = match HttpShortLinker::new(endpoint.into(), "token".into()) {
                Ok(_) => panic!("invalid endpoint should be rejected: {endpoint}"),
                Err(err) => err.to_string(),
            };
            assert!(err.contains("URL") || err.contains("host") || err.contains("scheme"));
        }
    }

    #[test]
    fn http_short_linker_posts_authorized_create_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/api/links", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&request);
                if let Some(headers_end) = text.find("\r\n\r\n") {
                    let headers = &text[..headers_end];
                    let content_length = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= headers_end + 4 + content_length {
                        break;
                    }
                }
            }

            let response = r#"{"short_url":"https://go.jczhang.cc/a1b2c3d4"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response.len(),
                response
            )
            .unwrap();
            String::from_utf8(request).unwrap()
        });

        let shortener = HttpShortLinker::new(endpoint, "secret-token".to_string()).unwrap();

        let short_url = shortener
            .shorten("https://s3.example.test/long?signature=abc", Some(3600))
            .unwrap();

        let request = server.join().unwrap();
        assert_eq!(short_url, "https://go.jczhang.cc/a1b2c3d4");
        assert!(request.starts_with("POST /api/links HTTP/1.1"));
        assert!(request.contains("Authorization: Bearer secret-token"));
        assert!(request.contains(r#""url":"https://s3.example.test/long?signature=abc""#));
        assert!(request.contains(r#""ttl_seconds":3600"#));
    }

    #[test]
    fn http_short_linker_rejects_insecure_response_url() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/api/links", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer).unwrap();
            let response = r#"{"short_url":"http://example.com/not-ok"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response.len(),
                response
            )
            .unwrap();
        });

        let shortener = HttpShortLinker::new(endpoint, "secret-token".to_string()).unwrap();
        let err = shortener
            .shorten("https://s3.example.test/long?signature=abc", Some(3600))
            .unwrap_err()
            .to_string();

        server.join().unwrap();
        assert!(err.contains("short_url") || err.contains("https://"));
    }
}
