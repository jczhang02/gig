//! Uploader trait, short links, and the backends: S3 (ported from v1;
//! credentials arrive through `secrets::ResolvedS3`) and bdpan (the Baidu
//! Netdisk CLI as a subprocess).

pub mod bdpan;
pub mod s3;

pub use bdpan::BdpanUploader;
pub use s3::S3Uploader;

use crate::config::{Config, Delivery, Paths};
use crate::models::Channel;
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

/// The configured default uploader (`delivery.uploader`); see
/// [`uploader_by_name`].
pub fn configured_uploader(config: &Config, paths: &Paths) -> Result<Box<dyn Uploader>> {
    uploader_by_name(config, paths, &config.delivery.uploader)
}

/// An uploader's name. Its text form (`bdpan`, or `s3:<name>` for a
/// `[delivery.s3.<name>]` table) is what `delivery.uploader`, `--uploader`
/// and the database hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploaderName {
    Bdpan,
    S3(String),
}

impl UploaderName {
    /// Read the text form. Anything else, also "", is a config error.
    pub fn parse(name: &str) -> Result<Self> {
        if name == bdpan::NAME {
            return Ok(Self::Bdpan);
        }
        match name.strip_prefix("s3:") {
            Some(s3) if !s3.is_empty() => Ok(Self::S3(s3.to_string())),
            _ => Err(Error::Config(if name.is_empty() {
                "no uploader is set (delivery.uploader is empty); use bdpan or s3:<name>".into()
            } else {
                format!("unknown uploader {name:?}; use bdpan or s3:<name>")
            })),
        }
    }

    /// The channel an upload through this uploader records.
    pub fn channel(&self) -> Channel {
        match self {
            Self::Bdpan => Channel::Pan,
            Self::S3(_) => Channel::Oss,
        }
    }
}

impl fmt::Display for UploaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bdpan => f.write_str(bdpan::NAME),
            Self::S3(name) => write!(f, "s3:{name}"),
        }
    }
}

/// Check that `name` is an uploader the config can build, without building
/// it or running anything: an `s3:<name>` also needs its
/// `[delivery.s3.<name>]` table. Anything else is a config error.
pub fn check_uploader_name(config: &Config, name: &str) -> Result<UploaderName> {
    let parsed = UploaderName::parse(name)?;
    if let UploaderName::S3(s3) = &parsed {
        config.s3_table(s3)?;
    }
    Ok(parsed)
}

/// Build the uploader called `name` (see [`check_uploader_name`]). Only S3
/// is wrapped with the short linker (when short links are enabled); a Pan
/// Share is never shortened.
pub fn uploader_by_name(config: &Config, paths: &Paths, name: &str) -> Result<Box<dyn Uploader>> {
    let s3_name = match check_uploader_name(config, name)? {
        UploaderName::Bdpan => return Ok(Box::new(BdpanUploader::new(&config.delivery))),
        UploaderName::S3(s3) => s3,
    };
    let resolved = secrets::resolve_s3_named(config, paths, &s3_name)?;
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

/// The uploaders an upload can switch between (the TUI's `Tab`), in that
/// order: `bdpan` when `delivery.uploader` names it, `[delivery.bdpan]`
/// changes its defaults, or its binary resolves; then `s3:<name>` for each
/// `[delivery.s3.<name>]` table.
pub fn uploader_choices(delivery: &Delivery) -> Vec<UploaderName> {
    uploader_choices_with(delivery, bdpan::bin_resolves)
}

/// [`uploader_choices`] with the binary lookup passed in.
fn uploader_choices_with(
    delivery: &Delivery,
    resolves: impl Fn(&str) -> bool,
) -> Vec<UploaderName> {
    let bdpan = UploaderName::parse(&delivery.uploader).ok() == Some(UploaderName::Bdpan)
        || delivery.bdpan != crate::config::Bdpan::default()
        || resolves(&delivery.bdpan.bin);
    bdpan
        .then_some(UploaderName::Bdpan)
        .into_iter()
        .chain(delivery.s3.keys().cloned().map(UploaderName::S3))
        .collect()
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

    fn config_with_s3_and_short_links(dir: &Path) -> (Config, Paths) {
        let paths = Paths::under_root(dir);
        let mut cfg = Config::default();
        cfg.delivery.uploader = "s3:default".into();
        for name in ["default", "hk"] {
            cfg.delivery.s3.insert(
                name.into(),
                crate::config::S3 {
                    bucket: "b".into(),
                    region: "r".into(),
                    endpoint: "https://s3.example.test".into(),
                    ..Default::default()
                },
            );
        }
        cfg.delivery.short_link.enabled = true;
        cfg.delivery.short_link.endpoint = "https://go.example.test/api".into();
        (cfg, paths)
    }

    fn by_name_err(cfg: &Config, paths: &Paths, name: &str) -> Error {
        match uploader_by_name(cfg, paths, name) {
            Ok(u) => panic!("{name} should be refused, built {}", u.name()),
            Err(e) => e,
        }
    }

    #[test]
    fn bdpan_is_built_without_short_link_secrets() {
        let dir = tempfile::tempdir().unwrap();
        // Short links on, but no token and no S3 keys anywhere.
        let (cfg, paths) = config_with_s3_and_short_links(dir.path());
        let u = uploader_by_name(&cfg, &paths, "bdpan").unwrap();
        assert_eq!(u.name(), "bdpan");
    }

    #[test]
    fn s3_by_name_overrides_the_configured_default() {
        let dir = tempfile::tempdir().unwrap();
        let (mut cfg, paths) = config_with_s3_and_short_links(dir.path());
        cfg.delivery.short_link.enabled = false;
        secrets::write_file(
            &paths.secrets_file,
            "[s3.hk]\naccess_key = \"AK\"\nsecret_key = \"SK\"\n",
        )
        .unwrap();
        assert_eq!(
            uploader_by_name(&cfg, &paths, "s3:hk").unwrap().name(),
            "s3:hk"
        );
        // The default has no keys: still the secrets error it always was.
        assert_eq!(
            match configured_uploader(&cfg, &paths) {
                Ok(_) => panic!("s3:default has no keys"),
                Err(e) => e.code(),
            },
            "secrets"
        );
        // S3 still needs the short link token when short links are on.
        cfg.delivery.short_link.enabled = true;
        assert_eq!(by_name_err(&cfg, &paths, "s3:hk").code(), "secrets");
    }

    #[test]
    fn uploader_choices_list_bdpan_when_named_or_found_then_each_s3_table() {
        let dir = tempfile::tempdir().unwrap();
        let (cfg, _) = config_with_s3_and_short_links(dir.path());
        let found = |_: &str| true;
        let missing = |_: &str| false;
        // The choices as text, the way the TUI shows them.
        fn uploader_choices_with(d: &Delivery, resolves: impl Fn(&str) -> bool) -> Vec<String> {
            super::uploader_choices_with(d, resolves)
                .iter()
                .map(ToString::to_string)
                .collect()
        }
        // Default bdpan settings, binary not found, not the default: S3 only.
        assert_eq!(
            uploader_choices_with(&cfg.delivery, missing),
            ["s3:default", "s3:hk"]
        );
        // Found on PATH (or as the configured path): bdpan comes first.
        assert_eq!(
            uploader_choices_with(&cfg.delivery, found),
            ["bdpan", "s3:default", "s3:hk"]
        );
        // Named as the default, or configured in [delivery.bdpan]: offered
        // even when not found, so the upload says why it fails.
        let mut named = cfg.delivery.clone();
        named.uploader = "bdpan".into();
        assert_eq!(
            uploader_choices_with(&named, missing),
            ["bdpan", "s3:default", "s3:hk"]
        );
        let mut table = cfg.delivery.clone();
        table.bdpan.bin = "/opt/bdpan/bin/bdpan".into();
        assert_eq!(uploader_choices_with(&table, missing)[0], "bdpan");
        // The lookup is asked about the configured binary name.
        let asked = std::cell::RefCell::new(Vec::new());
        uploader_choices_with(&cfg.delivery, |b: &str| {
            asked.borrow_mut().push(b.to_string());
            false
        });
        assert_eq!(asked.into_inner(), ["bdpan"]);
        // Nothing configured, nothing found.
        assert!(uploader_choices_with(&Config::default().delivery, missing).is_empty());
    }

    #[test]
    fn uploader_names_round_trip_and_pick_the_channel() {
        for (text, name, channel) in [
            ("bdpan", UploaderName::Bdpan, Channel::Pan),
            (
                "s3:aliyun-bj",
                UploaderName::S3("aliyun-bj".into()),
                Channel::Oss,
            ),
        ] {
            assert_eq!(UploaderName::parse(text).unwrap(), name);
            assert_eq!(name.to_string(), text);
            assert_eq!(name.channel(), channel);
        }
        for text in ["", "s3:", "nope", "bdpan:x", "BDPAN"] {
            let err = UploaderName::parse(text).unwrap_err();
            assert_eq!(err.code(), "config", "{text}");
            assert!(err.to_string().contains("use bdpan or s3:<name>"), "{err}");
        }
    }

    #[test]
    fn unknown_uploader_names_are_config_errors() {
        let dir = tempfile::tempdir().unwrap();
        let (cfg, paths) = config_with_s3_and_short_links(dir.path());
        for name in ["", "nope", "s3:", "s3:missing", "bdpan:work", "BDPAN"] {
            assert_eq!(by_name_err(&cfg, &paths, name).code(), "config", "{name}");
        }
        assert!(by_name_err(&cfg, &paths, "nope")
            .to_string()
            .contains("bdpan or s3:<name>"));
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
