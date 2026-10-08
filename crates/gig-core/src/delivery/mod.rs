//! Uploader trait, short links, and the S3 backend. Ported from v1; the only
//! change is that credentials arrive through `secrets::ResolvedS3`.

pub mod s3;

pub use s3::S3Uploader;

use crate::config::{Config, Paths};
use crate::secrets;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;

/// Upload progress callback: `(bytes sent, total bytes)`.
pub type Progress = Arc<dyn Fn(u64, u64) + Send + Sync>;

#[derive(Clone, Default)]
pub struct UploadOpts {
    /// Backend object key; the local basename when None.
    pub object_key: Option<String>,
    /// Called by the uploader as bytes go out: after each multipart part, and
    /// once when a single PUT has finished. The CLI passes None.
    pub progress: Option<Progress>,
}

impl UploadOpts {
    /// Report progress when a callback is set.
    pub fn report(&self, sent: u64, total: u64) {
        if let Some(p) = &self.progress {
            p(sent, total);
        }
    }
}

impl fmt::Debug for UploadOpts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadOpts")
            .field("object_key", &self.object_key)
            .field("progress", &self.progress.as_ref().map(|_| ".."))
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct UploadResult {
    pub url: String,
    /// The shortened link, when a short linker ran.
    pub short_url: Option<String>,
    /// Unix seconds.
    pub expires_at: Option<i64>,
    pub provider: String,
    pub file_size: u64,
    /// The extraction code of a Pan Share, also embedded in `url`.
    pub pwd: Option<String>,
}

pub trait Uploader: Send + Sync {
    fn name(&self) -> &str;
    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult>;
}

pub trait ShortLinker: Send + Sync {
    fn shorten(&self, long_url: &str, ttl_seconds: Option<u32>) -> Result<String>;
}

pub struct ShorteningUploader {
    inner: Box<dyn Uploader>,
    short_linker: Box<dyn ShortLinker>,
}

impl ShorteningUploader {
    pub fn new(inner: Box<dyn Uploader>, short_linker: Box<dyn ShortLinker>) -> Self {
        Self {
            inner,
            short_linker,
        }
    }
}

impl Uploader for ShorteningUploader {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
        let mut result = self.inner.upload(local, opts)?;
        let ttl = ttl_from_expiry(result.expires_at);
        result.short_url = Some(self.short_linker.shorten(&result.url, ttl)?);
        Ok(result)
    }
}

fn ttl_from_expiry(expires_at: Option<i64>) -> Option<u32> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    u32::try_from(expires_at?.checked_sub(now)?).ok()
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

impl HttpShortLinker {
    pub fn new(endpoint: String, token: String) -> Result<Self> {
        if endpoint.trim().is_empty() {
            return Err(Error::Config("short link endpoint is empty".into()));
        }
        if token.trim().is_empty() {
            return Err(Error::Secrets("short link token is empty".into()));
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
            .map_err(|e| Error::Upload(format!("short link request failed: {e}")))?
            .into_json()
            .map_err(|e| Error::Upload(format!("short link response invalid: {e}")))?;
        if response.short_url.trim().is_empty() {
            return Err(Error::Upload(
                "short link response missing short_url".into(),
            ));
        }
        validate_https_or_loopback_url(&response.short_url, "short link response short_url")?;
        Ok(response.short_url)
    }
}

/// The configured uploader, wrapped with the short linker when enabled.
pub fn configured_uploader(config: &Config, paths: &Paths) -> Result<Box<dyn Uploader>> {
    let resolved = secrets::resolve_s3(config, paths)?;
    let s3 = S3Uploader::new(&resolved)?;
    match secrets::resolve_short_link_token(config, paths)? {
        Some(token) => {
            let linker = HttpShortLinker::new(config.delivery.short_link.endpoint.clone(), token)?;
            Ok(Box::new(ShorteningUploader::new(
                Box::new(s3),
                Box::new(linker),
            )))
        }
        None => Ok(Box::new(s3)),
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
        "{label} must use https:// unless it targets loopback or insecure HTTP is explicitly enabled"
    )))
}

struct ParsedUrl<'a> {
    scheme: &'a str,
    authority: &'a str,
}

impl<'a> ParsedUrl<'a> {
    fn parse(value: &'a str, label: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
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
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn host_from_authority(authority: &str) -> &str {
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(rest) = host_port.strip_prefix('[') {
        return rest.split(']').next().unwrap_or_default();
    }
    host_port.split(':').next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;

    struct StaticUploader;

    impl Uploader for StaticUploader {
        fn name(&self) -> &str {
            "test:s3"
        }
        fn upload(&self, _local: &Path, _opts: &UploadOpts) -> Result<UploadResult> {
            Ok(UploadResult {
                url: "https://s3.example.test/long?signature=abc".into(),
                short_url: None,
                expires_at: Some(OffsetDateTime::now_utc().unix_timestamp() + 3600),
                provider: "test:s3".into(),
                file_size: 123,
                pwd: None,
            })
        }
    }

    struct StaticLinker;

    impl ShortLinker for StaticLinker {
        fn shorten(&self, _long_url: &str, ttl: Option<u32>) -> Result<String> {
            assert!(ttl.is_some_and(|t| t > 3000 && t <= 3600));
            Ok("https://go.example.test/a1b2".into())
        }
    }

    #[test]
    fn shortening_uploader_keeps_long_url_and_adds_short() {
        let u = ShorteningUploader::new(Box::new(StaticUploader), Box::new(StaticLinker));
        let r = u
            .upload(&PathBuf::from("x.zip"), &UploadOpts::default())
            .unwrap();
        assert_eq!(r.url, "https://s3.example.test/long?signature=abc");
        assert_eq!(r.short_url.as_deref(), Some("https://go.example.test/a1b2"));
        assert_eq!(r.file_size, 123);
    }

    struct ReportingUploader;

    impl Uploader for ReportingUploader {
        fn name(&self) -> &str {
            "test:progress"
        }
        fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
            opts.report(1, 2);
            opts.report(2, 2);
            StaticUploader.upload(local, opts)
        }
    }

    #[test]
    fn progress_reaches_the_inner_uploader() {
        use std::sync::Mutex;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let opts = UploadOpts {
            object_key: None,
            progress: Some(Arc::new(move |sent, total| {
                sink.lock().unwrap().push((sent, total))
            })),
        };
        let u = ShorteningUploader::new(Box::new(ReportingUploader), Box::new(StaticLinker));
        u.upload(&PathBuf::from("x.zip"), &opts).unwrap();
        assert_eq!(*seen.lock().unwrap(), vec![(1, 2), (2, 2)]);
        // Without a callback, reporting is a no-op.
        UploadOpts::default().report(1, 1);
        assert!(format!("{opts:?}").contains("progress: Some"));
    }

    #[test]
    fn short_linker_rejects_plain_http_and_bad_urls() {
        assert!(HttpShortLinker::new("http://example.com/api".into(), "t".into()).is_err());
        for e in ["https://", "https:// bad.example", "go.example.test/api"] {
            assert!(HttpShortLinker::new(e.into(), "t".into()).is_err(), "{e}");
        }
        let err = match HttpShortLinker::new("https://go.example.test/api".into(), " ".into()) {
            Ok(_) => panic!("empty token should be rejected"),
            Err(e) => e,
        };
        assert_eq!(err.code(), "secrets");
    }

    #[test]
    fn short_linker_posts_bearer_request() {
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
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text[..end]
                        .lines()
                        .find_map(|l| l.strip_prefix("Content-Length: "))
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let body = r#"{"short_url":"https://go.example.test/a1b2c3d4"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            String::from_utf8(request).unwrap()
        });
        let linker = HttpShortLinker::new(endpoint, "secret-token".into()).unwrap();
        let short = linker
            .shorten("https://s3.example.test/long?sig=1", Some(3600))
            .unwrap();
        let request = server.join().unwrap();
        assert_eq!(short, "https://go.example.test/a1b2c3d4");
        assert!(request.starts_with("POST /api/links HTTP/1.1"));
        assert!(request.contains("Authorization: Bearer secret-token"));
        assert!(request.contains(r#""ttl_seconds":3600"#));
    }

    #[test]
    fn short_linker_rejects_insecure_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/api/links", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer).unwrap();
            let body = r#"{"short_url":"http://example.com/not-ok"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let linker = HttpShortLinker::new(endpoint, "t".into()).unwrap();
        assert!(linker.shorten("https://s3.example.test/x", None).is_err());
        server.join().unwrap();
    }
}
