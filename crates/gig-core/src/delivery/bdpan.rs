//! The `bdpan` uploader: drives the official Baidu Netdisk CLI as a
//! subprocess, uploads the file and returns a Pan Share link with the
//! extraction code embedded. bdpan owns its own login.

#[cfg(all(unix, any(test, feature = "test-util")))]
pub mod fake;

use crate::config::Delivery;
use crate::delivery::{UploadOpts, UploadResult, Uploader};
use crate::{Error, Result};
use serde_json::Value;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use time::OffsetDateTime;

const DAY_SECONDS: u64 = 86_400;

/// Timeout for `whoami` and `share`. `upload` has none.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

const LOGIN_HINT: &str = "run `! bdpan login`";

/// Share periods bdpan accepts, in days. 0 (permanent) is never derived.
const SHARE_PERIODS: [u32; 3] = [1, 7, 30];

/// The smallest share period that covers `ttl_seconds`, capped at 30 days.
pub fn share_period_days(ttl_seconds: u32) -> u32 {
    SHARE_PERIODS
        .into_iter()
        .find(|d| u64::from(*d) * DAY_SECONDS >= u64::from(ttl_seconds))
        .unwrap_or(30)
}

/// Uploads through the `bdpan` CLI to `<remote_root>/<object_key>` and
/// shares the file. Every call is `<bin> --json --no-check-update <args>`.
pub struct BdpanUploader {
    bin: String,
    remote_root: String,
    link_ttl_seconds: u32,
    call_timeout: Duration,
}

impl BdpanUploader {
    pub fn new(delivery: &Delivery) -> Self {
        Self {
            bin: delivery.bdpan.bin.clone(),
            remote_root: delivery.bdpan.remote_root.clone(),
            link_ttl_seconds: delivery.link_ttl_seconds,
            call_timeout: CALL_TIMEOUT,
        }
    }

    /// Override the 60 s timeout of `whoami` and `share` (tests).
    pub fn with_call_timeout(mut self, timeout: Duration) -> Self {
        self.call_timeout = timeout;
        self
    }

    /// The remote path relative to the bdpan app root.
    fn remote_path(&self, local: &Path, opts: &UploadOpts) -> Result<String> {
        let key = match &opts.object_key {
            Some(k) => k.trim_matches('/').to_string(),
            None => local
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
        if key.is_empty() {
            return Err(Error::InvalidInput(format!(
                "no remote name for {}",
                local.display()
            )));
        }
        let root = self.remote_root.trim_matches('/');
        Ok(if root.is_empty() {
            key
        } else {
            format!("{root}/{key}")
        })
    }

    fn ensure_logged_in(&self) -> Result<()> {
        let reply = match self.call(&["whoami"], Some(self.call_timeout)) {
            Ok(v) => v,
            Err(CallError::Failed(m)) => {
                return Err(Error::Secrets(format!(
                    "bdpan whoami failed ({m}); {LOGIN_HINT}"
                )))
            }
            Err(e) => return Err(e.into_error(&self.bin, "whoami")),
        };
        let flag = |k: &str| reply.get(k).and_then(Value::as_bool).unwrap_or(false);
        if flag("authenticated") && flag("has_valid_token") {
            Ok(())
        } else {
            Err(Error::Secrets(format!(
                "bdpan is not logged in or its token has expired; {LOGIN_HINT}"
            )))
        }
    }

    /// Share `remote` for `days`; returns `(link, pwd)`.
    fn share(&self, remote: &str, days: u32) -> Result<(String, Option<String>)> {
        let failed = |m: String| {
            Error::Upload(format!(
                "uploaded to bdpan at {remote} but sharing it failed: {m}"
            ))
        };
        let reply = self
            .call(
                &["share", remote, "--period", &days.to_string()],
                Some(self.call_timeout),
            )
            .map_err(|e| failed(e.message()))?;
        share_details(&reply).ok_or_else(|| failed("the reply has no https share link".into()))
    }

    /// Run bdpan with `args` and return its JSON reply when it succeeded.
    fn call(
        &self,
        args: &[&str],
        timeout: Option<Duration>,
    ) -> std::result::Result<Value, CallError> {
        let mut child = Command::new(&self.bin)
            .args(["--json", "--no-check-update"])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => CallError::NotFound,
                _ => CallError::Failed(format!("cannot run it: {e}")),
            })?;
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let status = wait(&mut child, timeout)?;
        let stdout = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        classify(status, &stdout, &stderr)
    }
}

impl Uploader for BdpanUploader {
    fn name(&self) -> &str {
        "bdpan"
    }

    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
        let file_size = std::fs::metadata(local)
            .map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))?
            .len();
        let remote = self.remote_path(local, opts)?;
        self.ensure_logged_in()?;
        let local_arg = local.to_string_lossy();
        // No timeout: a large upload takes as long as it takes. bdpan creates
        // missing parent directories itself (verified live with 3.8.7).
        match self.call(&["upload", &local_arg, &remote], None) {
            Ok(_) => {}
            Err(CallError::Failed(m)) => {
                return Err(Error::Upload(format!("bdpan upload to {remote}: {m}")))
            }
            Err(e) => return Err(e.into_error(&self.bin, "upload")),
        }
        opts.report(file_size, file_size);
        let days = share_period_days(self.link_ttl_seconds);
        let (link, pwd) = self.share(&remote, days)?;
        let url = match &pwd {
            Some(p) => {
                let sep = if link.contains('?') { '&' } else { '?' };
                format!("{link}{sep}pwd={p}")
            }
            None => link,
        };
        let expires_at =
            OffsetDateTime::now_utc().unix_timestamp() + i64::from(days) * DAY_SECONDS as i64;
        Ok(UploadResult {
            url,
            short_url: None,
            expires_at: Some(expires_at),
            provider: "bdpan".into(),
            file_size,
            pwd,
        })
    }
}

/// Why a bdpan call did not give a usable reply. Messages are redacted.
enum CallError {
    NotFound,
    TimedOut(Duration),
    Failed(String),
}

impl CallError {
    fn message(&self) -> String {
        match self {
            CallError::NotFound => "binary not found".into(),
            CallError::TimedOut(t) => format!("timed out after {} s", t.as_secs()),
            CallError::Failed(m) => m.clone(),
        }
    }

    fn into_error(self, bin: &str, subcommand: &str) -> Error {
        match self {
            CallError::NotFound => Error::Config(format!(
                "bdpan binary {bin:?} not found; install bdpan or set delivery.bdpan.bin"
            )),
            other => Error::Upload(format!("bdpan {subcommand}: {}", other.message())),
        }
    }
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    })
}

fn wait(
    child: &mut Child,
    timeout: Option<Duration>,
) -> std::result::Result<ExitStatus, CallError> {
    let io_err = |e: io::Error| CallError::Failed(format!("waiting for it failed: {e}"));
    let Some(timeout) = timeout else {
        return child.wait().map_err(io_err);
    };
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(io_err)? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CallError::TimedOut(timeout));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// A reply is good when the exit code is 0, stdout is JSON, and the JSON
/// reports no business error (bdpan exits 0 on some failures).
fn classify(
    status: ExitStatus,
    stdout: &str,
    stderr: &str,
) -> std::result::Result<Value, CallError> {
    let parsed: Option<Value> = serde_json::from_str(stdout.trim()).ok();
    let problem = match &parsed {
        Some(v) => business_error(v),
        None => None,
    };
    if status.success() && problem.is_none() {
        if let Some(v) = parsed {
            return Ok(v);
        }
    }
    let detail = problem
        .or_else(|| stderr_error(stderr))
        .unwrap_or_else(|| match status.code() {
            Some(0) => "the reply is not JSON".into(),
            Some(c) => format!("exit status {c}"),
            None => "killed by a signal".into(),
        });
    Err(CallError::Failed(redact(&detail)))
}

/// Port of `business_errors` from the verification script: a non-zero
/// `code`/`errno`/`error_code`, `success: false` or a non-empty `error`,
/// also inside `data`. Returns the most useful message.
fn business_error(value: &Value) -> Option<String> {
    let obj = value.as_object()?;
    let mut found = Vec::new();
    if let Some(e) = obj.get("error").filter(|e| truthy(e)) {
        found.push(match e.as_str() {
            Some(s) => s.to_string(),
            None => e.to_string(),
        });
    }
    for key in ["code", "errno", "error_code"] {
        if let Some(v) = obj.get(key) {
            let ok = match v {
                Value::Null => true,
                Value::Number(n) => n.as_i64() == Some(0) || n.as_u64() == Some(0),
                Value::String(s) => s == "0" || s == "OK",
                _ => false,
            };
            if !ok {
                found.push(format!("{key}={v}"));
            }
        }
    }
    if obj.get("success") == Some(&Value::Bool(false)) && found.is_empty() {
        found.push("success=false".into());
    }
    if let Some(inner) = obj.get("data").and_then(business_error) {
        found.push(inner);
    }
    (!found.is_empty()).then(|| found.join("; "))
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// The first `Error:` line of stderr (bdpan prints usage text after it),
/// else the first non-empty line.
fn stderr_error(stderr: &str) -> Option<String> {
    let mut lines = stderr.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.clone().next()?;
    let line = lines.find(|l| l.starts_with("Error:")).unwrap_or(first);
    Some(line.trim_start_matches("Error:").trim().to_string())
}

/// `(link, pwd)` from a share reply, at the top level or under `data`.
fn share_details(value: &Value) -> Option<(String, Option<String>)> {
    let obj = value.as_object()?;
    if let Some(link) = obj.get("link").and_then(Value::as_str) {
        if !link.starts_with("https://") {
            return None;
        }
        let pwd = obj
            .get("pwd")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
            .map(str::to_string);
        return Some((link.to_string(), pwd));
    }
    share_details(obj.get("data")?)
}

/// Blank token-like values, as `redact()` in the verification script does:
/// `(access_token|refresh_token|authorization|bduss|stoken)[=":\s]+` then the
/// value up to whitespace or one of `&,"<>`.
fn redact(text: &str) -> String {
    const KEYS: [&str; 5] = [
        "access_token",
        "refresh_token",
        "authorization",
        "bduss",
        "stoken",
    ];
    let bytes = text.as_bytes();
    let lower = text.to_ascii_lowercase();
    let lower = lower.as_bytes();
    let is_sep = |b: u8| matches!(b, b'=' | b'"' | b':') || b.is_ascii_whitespace();
    let ends_value =
        |b: u8| b.is_ascii_whitespace() || matches!(b, b'&' | b',' | b'"' | b'<' | b'>');
    let mut out = String::with_capacity(text.len());
    let (mut i, mut copied) = (0, 0);
    while i < bytes.len() {
        if let Some(key) = KEYS.iter().find(|k| lower[i..].starts_with(k.as_bytes())) {
            let mut j = i + key.len();
            let sep_start = j;
            while j < bytes.len() && is_sep(bytes[j]) {
                j += 1;
            }
            let value_start = j;
            while j < bytes.len() && !ends_value(bytes[j]) {
                j += 1;
            }
            if value_start > sep_start && j > value_start {
                out.push_str(&text[copied..value_start]);
                out.push_str("<REDACTED>");
                copied = j;
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&text[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_period_is_the_smallest_baidu_period_that_covers_the_ttl() {
        let day = 86_400;
        for (ttl, days) in [
            (0, 1),
            (1, 1),
            (day, 1),
            (day + 1, 7),
            (604_800, 7),
            (604_801, 30),
            (30 * day, 30),
            (30 * day + 1, 30),
            (u32::MAX, 30),
        ] {
            assert_eq!(share_period_days(ttl), days, "ttl {ttl}");
        }
    }

    #[test]
    fn redact_matches_the_verification_script() {
        // Expected values from verify.py's redact() on the same inputs.
        for (input, want) in [
            ("BDUSS=abc123&x=1 ok", "BDUSS=<REDACTED>&x=1 ok"),
            (
                r#"{"access_token": "tok-1", "n": 2}"#,
                r#"{"access_token": "<REDACTED>", "n": 2}"#,
            ),
            ("Authorization: Bearer xyz", "Authorization: <REDACTED> xyz"),
            ("stoken:令牌值,rest", "stoken:<REDACTED>,rest"),
            ("no secrets here; token=1", "no secrets here; token=1"),
            ("access_token", "access_token"),
        ] {
            assert_eq!(redact(input), want, "{input}");
        }
    }

    /// Uploads one small file into a nested new directory and shares it for
    /// one day on the real account. Run with
    /// `GIG_LIVE_BDPAN=1 cargo test -p gig-core live_bdpan -- --ignored`.
    #[test]
    #[ignore = "uses the real bdpan account; set GIG_LIVE_BDPAN=1"]
    fn live_bdpan_upload_creates_parent_dirs_and_shares() {
        if std::env::var("GIG_LIVE_BDPAN").as_deref() != Ok("1") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("c.txt");
        std::fs::write(&local, b"gig live bdpan test, no client data\n").unwrap();
        let mut delivery = Delivery::default();
        if let Ok(bin) = std::env::var("GIG_DELIVERY_BDPAN_BIN") {
            delivery.bdpan.bin = bin;
        }
        delivery.link_ttl_seconds = 86_400;
        let stamp = OffsetDateTime::now_utc().unix_timestamp();
        let opts = UploadOpts {
            object_key: Some(format!("_live/{stamp}/a/b/c.txt")),
            progress: None,
        };
        let r = BdpanUploader::new(&delivery).upload(&local, &opts).unwrap();
        assert!(r.url.starts_with("https://pan.baidu.com/"));
        assert!(r.url.contains("pwd="));
        assert!(r.pwd.is_some());
        assert_eq!(r.file_size, 36);
    }

    #[cfg(unix)]
    mod with_fake_bin {
        use super::super::fake::{self, FakeBdpan, Reply};
        use super::super::*;
        use crate::delivery::{UploadOpts, Uploader};
        use std::path::PathBuf;
        use std::sync::{Arc, Mutex};
        use tempfile::TempDir;

        struct Setup {
            _dir: TempDir,
            fake: FakeBdpan,
            local: PathBuf,
            uploader: BdpanUploader,
        }

        fn setup() -> Setup {
            let dir = tempfile::tempdir().unwrap();
            let fake = FakeBdpan::install(dir.path()).unwrap();
            let local = dir.path().join("p-1.zip");
            std::fs::write(&local, b"12345").unwrap();
            let mut delivery = Delivery::default();
            delivery.bdpan.bin = fake.bin().to_string_lossy().into_owned();
            let uploader = BdpanUploader::new(&delivery);
            Setup {
                _dir: dir,
                fake,
                local,
                uploader,
            }
        }

        fn opts(key: &str) -> UploadOpts {
            UploadOpts {
                object_key: Some(key.into()),
                progress: None,
            }
        }

        #[test]
        fn uploads_shares_and_returns_the_pan_share() {
            let s = setup();
            let seen = Arc::new(Mutex::new(Vec::new()));
            let sink = seen.clone();
            let opts = UploadOpts {
                object_key: Some("acme/p-1/20261008T000000Z/p-1.zip".into()),
                progress: Some(Arc::new(move |a, b| sink.lock().unwrap().push((a, b)))),
            };
            let before = time::OffsetDateTime::now_utc().unix_timestamp();

            let r = s.uploader.upload(&s.local, &opts).unwrap();

            assert_eq!(s.uploader.name(), "bdpan");
            assert_eq!(r.url, format!("{}?pwd={}", fake::LINK, fake::PWD));
            assert_eq!(r.pwd.as_deref(), Some(fake::PWD));
            assert_eq!(r.short_url, None);
            assert_eq!(r.provider, "bdpan");
            assert_eq!(r.file_size, 5);
            let expires = r.expires_at.unwrap();
            assert!((before + 7 * 86_400..=before + 7 * 86_400 + 5).contains(&expires));
            assert_eq!(*seen.lock().unwrap(), vec![(5, 5)]);
            let remote = "gig/acme/p-1/20261008T000000Z/p-1.zip";
            assert_eq!(
                s.fake.calls(),
                vec![
                    "--json --no-check-update whoami".to_string(),
                    format!(
                        "--json --no-check-update upload {} {remote}",
                        s.local.display()
                    ),
                    format!("--json --no-check-update share {remote} --period 7"),
                ]
            );
        }

        #[test]
        fn without_an_object_key_the_basename_goes_under_the_remote_root() {
            let s = setup();
            s.uploader.upload(&s.local, &UploadOpts::default()).unwrap();
            assert!(s.fake.calls()[1].ends_with(" gig/p-1.zip"));
        }

        #[test]
        fn a_business_error_with_exit_0_fails_the_upload_and_skips_share() {
            let s = setup();
            s.fake
                .reply("upload", Reply::upload_business_error())
                .unwrap();
            let err = s.uploader.upload(&s.local, &opts("a/b.zip")).unwrap_err();
            assert_eq!(err.code(), "upload");
            let msg = err.to_string();
            assert!(msg.contains("本地路径不存在"), "{msg}");
            assert!(msg.contains("code=1"), "{msg}");
            assert!(msg.contains("gig/a/b.zip"), "{msg}");
            assert_eq!(s.fake.calls().len(), 2);
        }

        #[test]
        fn a_share_failure_after_upload_names_the_remote_path() {
            let s = setup();
            s.fake
                .reply("share", Reply::share_missing_remote())
                .unwrap();
            let err = s.uploader.upload(&s.local, &opts("a/b.zip")).unwrap_err();
            assert_eq!(err.code(), "upload");
            let msg = err.to_string();
            assert!(msg.contains("gig/a/b.zip"), "{msg}");
            assert!(msg.contains("文件或文件夹不存在"), "{msg}");
            assert!(!msg.contains("Usage"), "{msg}");
        }

        #[test]
        fn a_share_reply_without_a_link_is_a_share_failure() {
            let s = setup();
            s.fake
                .reply("share", Reply::json(serde_json::json!({"period": 7})))
                .unwrap();
            let err = s.uploader.upload(&s.local, &opts("a/b.zip")).unwrap_err();
            assert_eq!(err.code(), "upload");
            assert!(err.to_string().contains("gig/a/b.zip"));
        }

        #[test]
        fn not_logged_in_is_a_secrets_error_before_any_upload() {
            for reply in [
                Reply::whoami(false, false, "2026-01-01T00:00:00Z"),
                Reply::whoami(true, false, "2026-01-01T00:00:00Z"),
            ] {
                let s = setup();
                s.fake.reply("whoami", reply).unwrap();
                let err = s.uploader.upload(&s.local, &opts("a/b.zip")).unwrap_err();
                assert_eq!(err.code(), "secrets");
                assert!(err.to_string().contains("! bdpan login"), "{err}");
                assert_eq!(s.fake.calls().len(), 1);
            }
        }

        #[test]
        fn whoami_and_share_time_out() {
            let s = setup();
            let uploader = s
                .uploader
                .with_call_timeout(std::time::Duration::from_millis(300));
            s.fake.hang("share", 30).unwrap();
            let started = std::time::Instant::now();
            let err = uploader.upload(&s.local, &opts("a/b.zip")).unwrap_err();
            assert!(started.elapsed() < std::time::Duration::from_secs(10));
            assert_eq!(err.code(), "upload");
            assert!(err.to_string().contains("timed out"), "{err}");
            assert!(err.to_string().contains("gig/a/b.zip"), "{err}");

            s.fake.hang("whoami", 30).unwrap();
            let err = uploader.upload(&s.local, &opts("a/b.zip")).unwrap_err();
            assert!(err.to_string().contains("timed out"), "{err}");
        }

        #[test]
        fn a_missing_binary_is_a_config_error() {
            let s = setup();
            let mut delivery = Delivery::default();
            delivery.bdpan.bin = s
                .local
                .with_file_name("no-such-bdpan")
                .to_string_lossy()
                .into();
            let err = BdpanUploader::new(&delivery)
                .upload(&s.local, &opts("a/b.zip"))
                .unwrap_err();
            assert_eq!(err.code(), "config");
            assert!(err.to_string().contains("delivery.bdpan.bin"), "{err}");
        }

        #[test]
        fn error_text_from_bdpan_is_redacted() {
            let s = setup();
            s.fake
                .reply(
                    "upload",
                    Reply::fail(1, "Error: bad request access_token=abc123&x=1\n"),
                )
                .unwrap();
            let err = s.uploader.upload(&s.local, &opts("a/b.zip")).unwrap_err();
            let msg = err.to_string();
            assert!(!msg.contains("abc123"), "{msg}");
            assert!(msg.contains("access_token=<REDACTED>&x=1"), "{msg}");
        }
    }
}
