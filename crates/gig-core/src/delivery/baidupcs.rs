//! The `baidupcs` uploader: drives the `baidupcs-go` CLI as a subprocess,
//! uploads the file, checks its remote size, shares it and returns a Pan
//! Share link with the extraction code embedded. baidupcs-go owns its own
//! login (BDUSS + STOKEN); gig never reads Baidu credentials.
//!
//! baidupcs-go's exit codes are not reliable (it exits 0 on several
//! failures), so every step is judged by its text output.

#[cfg(all(unix, any(test, feature = "test-util")))]
pub mod fake;

use crate::config::Delivery;
use crate::delivery::subprocess::{self, redact, CallError};
use crate::delivery::{UploadOpts, UploadResult, Uploader};
use crate::{Error, Result};
use std::io::{self, Read};
use std::path::Path;
use std::process::ExitStatus;
use std::time::Duration;
use time::OffsetDateTime;

/// The uploader name (`delivery.uploader = "baidupcs"`) and result provider.
pub const NAME: &str = "baidupcs";

/// Timeout for `who`, `mkdir`, `meta` and `share`. `upload` has none.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// What to tell JC when baidupcs-go is not logged in, or lacks the STOKEN.
pub const LOGIN_HINT: &str = "run `! baidupcs-go login -bduss=<BDUSS> -stoken=<STOKEN>`";

const DAY_SECONDS: u32 = 86_400;

/// The share period for `ttl_seconds`: whole days rounded up, at least 1.
/// Never 0, which baidupcs-go takes as permanent.
pub fn share_period_days(ttl_seconds: u32) -> u32 {
    ttl_seconds.div_ceil(DAY_SECONDS).max(1)
}

/// A size as baidupcs-go prints it (`27.50MB`, `512B`, `50.000000MB`): a
/// non-negative number and one of B, KB, MB, GB, TB, base 1024, rounded to
/// whole bytes.
pub fn parse_size(text: &str) -> Option<u64> {
    const UNITS: [(&str, f64); 5] = [
        ("TB", 1_099_511_627_776.0),
        ("GB", 1_073_741_824.0),
        ("MB", 1_048_576.0),
        ("KB", 1024.0),
        ("B", 1.0),
    ];
    let (number, scale) = UNITS
        .iter()
        .find_map(|(unit, scale)| Some((text.strip_suffix(unit)?, *scale)))?;
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let value: f64 = number.parse().ok()?;
    Some((value * scale).round() as u64)
}

/// `(sent, total)` from an upload progress line,
/// `[1] ↑ <sent>/<total> <rate> in <time> ...`.
fn parse_progress(line: &str) -> Option<(u64, u64)> {
    let (_, rest) = line.split_once("↑ ")?;
    let (sent, total) = rest.split_whitespace().next()?.split_once('/')?;
    Some((parse_size(sent)?, parse_size(total)?))
}

/// The config error for a baidupcs-go binary that does not resolve.
pub(crate) fn not_found(bin: &str) -> Error {
    Error::Config(format!(
        "baidupcs-go binary {bin:?} not found; install baidupcs-go or set delivery.baidupcs.bin"
    ))
}

/// Uploads through `baidupcs-go` into `<remote_root>/<dir part of the
/// object key>/<local basename>` and shares the file.
pub struct BaiduPcsUploader {
    bin: String,
    remote_root: String,
    link_ttl_seconds: u32,
    call_timeout: Duration,
}

impl BaiduPcsUploader {
    pub fn new(delivery: &Delivery) -> Self {
        Self {
            bin: delivery.baidupcs.bin.clone(),
            remote_root: delivery.baidupcs.remote_root.clone(),
            link_ttl_seconds: delivery.link_ttl_seconds,
            call_timeout: CALL_TIMEOUT,
        }
    }

    /// Override the 60 s timeout of `who`, `mkdir`, `meta` and `share`
    /// (tests).
    pub fn with_call_timeout(mut self, timeout: Duration) -> Self {
        self.call_timeout = timeout;
        self
    }

    /// Check that baidupcs-go is logged in: `who` must name an account.
    fn who(&self) -> Result<()> {
        let out = subprocess::run(&self.bin, &["who"], Some(self.call_timeout))
            .map_err(|e| self.call_error(e, "who"))?;
        if has_account(&out.stdout) {
            Ok(())
        } else {
            Err(Error::Secrets(format!(
                "baidupcs-go is not logged in; {LOGIN_HINT}"
            )))
        }
    }

    /// Where `local` goes: `<remote_root>/<dir part of the object key>`,
    /// keeping the local basename (the key's last component for packages
    /// and artifacts).
    fn remote(&self, local: &Path, opts: &UploadOpts) -> Result<Remote> {
        let base = local
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| {
                Error::InvalidInput(format!("no remote name for {}", local.display()))
            })?;
        let key = opts.object_key.as_deref().unwrap_or_default();
        let key_dir = key
            .trim_matches('/')
            .rsplit_once('/')
            .map_or("", |(dir, _)| dir);
        let dir = [self.remote_root.trim_matches('/'), key_dir]
            .into_iter()
            .filter(|part| !part.is_empty())
            .fold(String::new(), |dir, part| format!("{dir}/{part}"));
        let file = format!("{dir}/{base}");
        let dir = if dir.is_empty() { "/".into() } else { dir };
        Ok(Remote { dir, file })
    }

    /// Create `dir` (with its parents); one that exists is fine.
    fn mkdir(&self, dir: &str) -> Result<()> {
        let what = format!("mkdir {dir}");
        let out = subprocess::run(&self.bin, &["mkdir", dir], Some(self.call_timeout))
            .map_err(|e| self.call_error(e, &what))?;
        let text = format!("{}\n{}", out.stdout, out.stderr);
        let exists = text.contains("已存在") || text.contains("31061");
        let failed = text.contains("失败") || text.contains("代码:");
        if out.status.success() && (exists || !failed) {
            return Ok(());
        }
        Err(Error::Upload(format!(
            "baidupcs-go {what} failed: {}",
            detail(out.status, &text)
        )))
    }

    /// Upload `local` into `remote.dir` with no timeout, reporting progress
    /// as baidupcs-go prints it.
    fn send(&self, local: &Path, remote: &Remote, opts: &UploadOpts) -> Result<()> {
        let local_arg = local.to_string_lossy();
        let mut child = subprocess::spawn(&self.bin, &["upload", &local_arg, &remote.dir])
            .map_err(|e| self.call_error(e, "upload"))?;
        let stderr = subprocess::drain(child.stderr.take());
        let mut lines = Vec::new();
        if let Some(stdout) = child.stdout.take() {
            for_each_line(stdout, |line| match parse_progress(line) {
                Some((sent, total)) => opts.report(sent, total),
                None => lines.push(line.to_string()),
            });
        }
        let status =
            subprocess::wait(&mut child, None).map_err(|e| self.call_error(e, "upload"))?;
        let stderr = stderr.join().unwrap_or_default();
        let stdout = lines.join("\n");
        if status.success() && lines.iter().any(|l| l.contains(UPLOADED)) {
            return Ok(());
        }
        if stdout.contains("STOKEN") || stderr.contains("STOKEN") {
            return Err(Error::Secrets(format!(
                "baidupcs-go cannot upload without the STOKEN in its login; {LOGIN_HINT}"
            )));
        }
        let detail = last_line(&stdout)
            .or_else(|| last_line(&stderr))
            .map(redact)
            .unwrap_or_else(|| exit_text(status));
        Err(Error::Upload(format!(
            "baidupcs-go upload to {} failed: {detail}",
            remote.file
        )))
    }

    /// Check that the remote file has `size` bytes.
    fn check_size(&self, file: &str, size: u64) -> Result<()> {
        let out = subprocess::run(&self.bin, &["meta", file], Some(self.call_timeout))
            .map_err(|e| self.call_error(e, &format!("meta {file}")))?;
        match meta_size(&out.stdout) {
            Some(n) if n == size => Ok(()),
            Some(n) => Err(Error::Upload(format!(
                "uploaded {file} but its remote size is {n} bytes, not {size}"
            ))),
            None => Err(Error::Upload(format!(
                "uploaded {file} but baidupcs-go meta gave no size: {}",
                detail(out.status, &format!("{}\n{}", out.stdout, out.stderr))
            ))),
        }
    }

    /// Share `file` for `days`.
    fn share(&self, file: &str, days: u32) -> Result<Share> {
        let period = days.to_string();
        let out = subprocess::run(
            &self.bin,
            &["share", "set", "--period", &period, "-f", file],
            Some(self.call_timeout),
        )
        .map_err(|e| self.call_error(e, &format!("share {file}")))?;
        parse_share(&out.stdout).ok_or_else(|| {
            Error::Upload(format!(
                "uploaded {file} but sharing it failed, no https://pan.baidu.com/ link with pwd=<code>: {}",
                detail(out.status, &format!("{}\n{}", out.stdout, out.stderr))
            ))
        })
    }

    /// The whole sequence; the share id is kept for the live test's
    /// cleanup.
    fn run(&self, local: &Path, opts: &UploadOpts) -> Result<(UploadResult, Share)> {
        let file_size = std::fs::metadata(local)
            .map_err(|e| Error::PathUnavailable(local.to_path_buf(), e))?
            .len();
        let remote = self.remote(local, opts)?;
        self.who()?;
        self.mkdir(&remote.dir)?;
        self.send(local, &remote, opts)?;
        self.check_size(&remote.file, file_size)?;
        let days = share_period_days(self.link_ttl_seconds);
        let shared_at = OffsetDateTime::now_utc().unix_timestamp();
        let share = self.share(&remote.file, days)?;
        opts.report(file_size, file_size);
        let result = UploadResult {
            url: share.url.clone(),
            short_url: None,
            expires_at: Some(shared_at + i64::from(days) * i64::from(DAY_SECONDS)),
            provider: NAME.into(),
            file_size,
            pwd: Some(share.pwd.clone()),
        };
        Ok((result, share))
    }

    /// A call that gave no output to judge, as the error the uploader
    /// returns.
    fn call_error(&self, e: CallError, what: &str) -> Error {
        match e {
            CallError::NotFound => not_found(&self.bin),
            CallError::NotExecutable => Error::Config(format!(
                "baidupcs-go binary {:?} is not executable; fix its mode or set delivery.baidupcs.bin",
                self.bin
            )),
            other => Error::Upload(format!("baidupcs-go {what}: {}", other.message())),
        }
    }
}

impl Uploader for BaiduPcsUploader {
    fn name(&self) -> &str {
        NAME
    }

    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
        self.run(local, opts).map(|(result, _)| result)
    }
}

/// The line baidupcs-go prints when the file is stored.
const UPLOADED: &str = "上传文件成功, 保存到网盘路径:";

/// Where an upload goes on the netdisk: the directory and the file in it.
struct Remote {
    dir: String,
    file: String,
}

/// What `share set` replied.
#[derive(Debug, PartialEq, Eq)]
struct Share {
    id: String,
    /// The full link, with `?pwd=<code>`.
    url: String,
    pwd: String,
}

/// The share in `shareID: <id>, 链接: <url>`, when the url is a
/// `https://pan.baidu.com/` link that carries `pwd=<code>`.
fn parse_share(text: &str) -> Option<Share> {
    text.lines().find_map(|line| {
        let (_, rest) = line.split_once("shareID:")?;
        let (id, rest) = rest.split_once(',')?;
        let (_, link) = rest.split_once("链接:")?;
        let url = link.split_whitespace().next()?;
        let (_, query) = url
            .strip_prefix("https://pan.baidu.com/")?
            .split_once('?')?;
        let pwd = query.split('&').find_map(|kv| kv.strip_prefix("pwd="))?;
        (!pwd.is_empty()).then(|| Share {
            id: id.trim().to_string(),
            url: url.to_string(),
            pwd: pwd.to_string(),
        })
    })
}

/// The byte count in `meta`'s `文件大小 <bytes>, <size>MB` line.
fn meta_size(text: &str) -> Option<u64> {
    text.split("文件大小").skip(1).find_map(|rest| {
        let rest = rest.trim_start();
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 || !rest[digits..].starts_with(',') {
            return None;
        }
        rest[..digits].parse().ok()
    })
}

/// Read `pipe` to the end, calling `f` with each line as it arrives. Lines
/// end at `\r` or `\n` (progress is redrawn with `\r`); blank lines are
/// skipped.
fn for_each_line(mut pipe: impl Read, mut f: impl FnMut(&str)) {
    let mut emit = |bytes: &[u8]| {
        let line = String::from_utf8_lossy(bytes);
        let line = line.trim();
        if !line.is_empty() {
            f(line);
        }
    };
    let mut pending = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = match pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        for &b in &buf[..n] {
            if b == b'\r' || b == b'\n' {
                emit(&pending);
                pending.clear();
            } else {
                pending.push(b);
            }
        }
    }
    emit(&pending);
}

/// The last line of `text` that says something: not blank, not progress,
/// not the closing `上传结束` summary.
fn last_line(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty() && !l.starts_with("上传结束") && parse_progress(l).is_none())
}

/// The exit status in words.
fn exit_text(status: ExitStatus) -> String {
    match status.code() {
        Some(c) => format!("exit status {c}"),
        None => "killed by a signal".into(),
    }
}

/// What went wrong, for a message: the last line of `text`, redacted, or
/// the exit status.
fn detail(status: ExitStatus, text: &str) -> String {
    last_line(text)
        .map(redact)
        .unwrap_or_else(|| exit_text(status))
}

/// True when `who` names an account: `当前帐号 uid: <uid>` with a uid
/// other than 0 (what baidupcs-go prints when no account is active).
fn has_account(who: &str) -> bool {
    who.split("当前帐号 uid:").skip(1).any(|rest| {
        let uid: String = rest
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        uid.chars().any(|c| c != '0')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_parse_in_base_1024() {
        for (text, bytes) in [
            ("0B", Some(0)),
            ("512B", Some(512)),
            ("1.50KB", Some(1536)),
            ("27.50MB", Some(28_835_840)),
            // 43.44 * 1048576 = 45550141.44
            ("43.44MB", Some(45_550_141)),
            ("50.00MB", Some(52_428_800)),
            ("50.000000MB", Some(52_428_800)),
            ("1.00GB", Some(1_073_741_824)),
            ("32.70GB", Some(35_111_357_645)),
            ("2TB", Some(2_199_023_255_552)),
            ("", None),
            ("MB", None),
            ("12", None),
            ("1.5PB", None),
            ("1.5mb", None),
            ("-1MB", None),
            ("x1MB", None),
            ("9.61MB/s", None),
        ] {
            assert_eq!(parse_size(text), bytes, "{text:?}");
        }
    }

    #[test]
    fn progress_lines_give_sent_and_total() {
        for (line, want) in [
            (
                "[1] ↑ 27.50MB/50.00MB 9.61MB/s in 3s ............",
                Some((28_835_840, 52_428_800)),
            ),
            (
                "[12] ↑ 1.00GB/32.70GB 18.20MB/s in 1m2s .....",
                Some((1_073_741_824, 35_111_357_645)),
            ),
            ("[1] ↑ 0B/50.00MB 0B/s in 0s", Some((0, 52_428_800))),
            ("[1] 开始上传文件...", None),
            ("[1] ↑ 27.50MB 9.61MB/s", None),
            ("[1] ↑ abc/50.00MB", None),
            ("", None),
        ] {
            assert_eq!(parse_progress(line), want, "{line:?}");
        }
    }

    #[test]
    fn share_period_is_the_ttl_in_whole_days_rounded_up_never_0() {
        let day = 86_400;
        for (ttl, days) in [
            (0, 1),
            (1, 1),
            (day, 1),
            (day + 1, 2),
            (604_800, 7),
            (30 * day + 1, 31),
            (u32::MAX, 49_711),
        ] {
            assert_eq!(share_period_days(ttl), days, "ttl {ttl}");
        }
    }

    #[cfg(unix)]
    mod with_fake_bin {
        use super::super::fake::{self, FakeBaiduPcs, Reply};
        use super::super::*;
        use std::path::PathBuf;
        use std::sync::{Arc, Mutex};
        use tempfile::TempDir;

        struct Setup {
            _dir: TempDir,
            fake: FakeBaiduPcs,
            local: PathBuf,
            uploader: BaiduPcsUploader,
        }

        /// A fake bin and a local `p-1.zip` of [`fake::SIZE`] bytes (sparse).
        fn setup() -> Setup {
            let dir = tempfile::tempdir().unwrap();
            let fake = FakeBaiduPcs::install(dir.path()).unwrap();
            let local = dir.path().join("p-1.zip");
            std::fs::File::create(&local)
                .unwrap()
                .set_len(fake::SIZE)
                .unwrap();
            let mut delivery = Delivery::default();
            delivery.baidupcs.bin = fake.bin().to_string_lossy().into_owned();
            let uploader = BaiduPcsUploader::new(&delivery);
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

        /// Progress reports collected by an [`UploadOpts`] callback.
        type Seen = Arc<Mutex<Vec<(u64, u64)>>>;

        /// Options for `key` whose callback records each report and, after
        /// the third, creates `gate` (see [`FakeBaiduPcs::hold_upload`]).
        fn opts_with_progress(key: &str, gate: PathBuf) -> (UploadOpts, Seen) {
            let seen = Seen::default();
            let sink = seen.clone();
            let opts = UploadOpts {
                object_key: Some(key.into()),
                progress: Some(Arc::new(move |sent, total| {
                    let mut seen = sink.lock().unwrap();
                    seen.push((sent, total));
                    if seen.len() == 3 {
                        std::fs::write(&gate, "").unwrap();
                    }
                })),
            };
            (opts, seen)
        }

        #[test]
        fn uploads_checks_the_size_shares_and_reports_progress_while_bytes_go_out() {
            let s = setup();
            // The upload prints its success line only once the third
            // progress report has arrived: buffering until exit fails.
            let gate = s._dir.path().join("third-report-seen");
            s.fake.hold_upload(&gate).unwrap();
            let (opts, seen) = opts_with_progress("acme/p-1/20261008T000000Z/p-1.zip", gate);
            let before = time::OffsetDateTime::now_utc().unix_timestamp();

            let r = s.uploader.upload(&s.local, &opts).unwrap();

            assert_eq!(s.uploader.name(), "baidupcs");
            assert_eq!(r.url, format!("{}?pwd={}", fake::LINK, fake::PWD));
            assert_eq!(r.pwd.as_deref(), Some(fake::PWD));
            assert_eq!(r.short_url, None);
            assert_eq!(r.provider, "baidupcs");
            assert_eq!(r.file_size, fake::SIZE);
            let expires = r.expires_at.unwrap();
            assert!((before + 7 * 86_400..=before + 7 * 86_400 + 5).contains(&expires));
            // 27.50 MB, 43.44 MB (45550141.44 bytes), 50 MB, then the final
            // report.
            assert_eq!(
                *seen.lock().unwrap(),
                [
                    (28_835_840, 52_428_800),
                    (45_550_141, 52_428_800),
                    (52_428_800, 52_428_800),
                    (52_428_800, 52_428_800),
                ]
            );
            let dir = "/gig/acme/p-1/20261008T000000Z";
            assert_eq!(
                s.fake.calls(),
                [
                    "who".to_string(),
                    format!("mkdir {dir}"),
                    format!("upload {} {dir}", s.local.display()),
                    format!("meta {dir}/p-1.zip"),
                    format!("share set --period 7 -f {dir}/p-1.zip"),
                ]
            );
        }

        /// The error of an upload of `a/p-1.zip` after `subcommand` was set
        /// to reply `reply`, and the subcommands called.
        fn failure(subcommand: &str, reply: Reply) -> (Error, Vec<String>) {
            let s = setup();
            s.fake.reply(subcommand, reply).unwrap();
            let err = s.uploader.upload(&s.local, &opts("a/p-1.zip")).unwrap_err();
            let subs = s
                .fake
                .calls()
                .iter()
                .map(|c| c.split(' ').next().unwrap_or_default().to_string())
                .collect();
            (err, subs)
        }

        #[test]
        fn mkdir_on_an_existing_dir_is_fine() {
            let s = setup();
            s.fake.reply("mkdir", Reply::mkdir_exists()).unwrap();
            let r = s.uploader.upload(&s.local, &opts("a/p-1.zip")).unwrap();
            assert_eq!(r.pwd.as_deref(), Some(fake::PWD));
        }

        #[test]
        fn a_mkdir_that_fails_otherwise_stops_before_the_upload() {
            for reply in [
                Reply::out(
                    0,
                    "创建目录 /gig/a 失败, 遇到错误, 远端服务器返回错误, 代码: 31062, 消息: 文件名非法\n",
                ),
                Reply::out(1, "网络错误\n"),
            ] {
                let (err, subs) = failure("mkdir", reply);
                assert_eq!(err.code(), "upload", "{err}");
                assert!(err.to_string().contains("mkdir /gig/a"), "{err}");
                assert_eq!(subs, ["who", "mkdir"]);
            }
        }

        #[test]
        fn an_upload_skipped_without_the_success_line_fails() {
            let (err, subs) = failure("upload", Reply::upload_skipped());
            assert_eq!(err.code(), "upload", "{err}");
            let msg = err.to_string();
            assert!(msg.contains("/gig/a/p-1.zip"), "{msg}");
            // The last meaningful line, not the closing summary.
            assert!(msg.contains("已存在, 跳过"), "{msg}");
            assert!(!msg.contains("上传结束"), "{msg}");
            assert_eq!(subs, ["who", "mkdir", "upload"]);
        }

        #[test]
        fn a_success_line_with_a_failing_exit_is_a_failure() {
            let mut reply = Reply::upload_ok();
            reply.exit = 1;
            let (err, _) = failure("upload", reply);
            assert_eq!(err.code(), "upload", "{err}");
        }

        #[test]
        fn an_upload_without_stoken_is_a_secrets_error_with_the_login_hint() {
            let (err, subs) = failure("upload", Reply::upload_missing_stoken());
            assert_eq!(err.code(), "secrets", "{err}");
            assert!(
                err.to_string()
                    .contains("! baidupcs-go login -bduss=<BDUSS> -stoken=<STOKEN>"),
                "{err}"
            );
            assert_eq!(subs, ["who", "mkdir", "upload"]);
        }

        #[test]
        fn upload_error_text_is_redacted() {
            let (err, _) = failure(
                "upload",
                Reply::out(1, "[1] 上传文件失败, 请求错误 BDUSS=abc123&x=1\n"),
            );
            let msg = err.to_string();
            assert!(!msg.contains("abc123"), "{msg}");
            assert!(msg.contains("BDUSS=<REDACTED>&x=1"), "{msg}");
        }

        #[test]
        fn a_remote_size_that_differs_fails_naming_the_remote_path() {
            for reply in [
                Reply::meta(fake::SIZE - 1),
                // `meta` of a missing path: exit 0 and an error text.
                Reply::out(
                    0,
                    "获取文件信息失败, 遇到错误, 远端服务器返回错误, 代码: 31066, 消息: 文件或目录不存在\n",
                ),
            ] {
                let (err, subs) = failure("meta", reply);
                assert_eq!(err.code(), "upload", "{err}");
                assert!(err.to_string().contains("/gig/a/p-1.zip"), "{err}");
                assert_eq!(subs, ["who", "mkdir", "upload", "meta"]);
            }
        }

        #[test]
        fn a_share_without_a_pan_link_and_pwd_fails_naming_the_remote_path() {
            for reply in [
                Reply::share(fake::LINK),
                Reply::share(&format!("{}?pwd=", fake::LINK)),
                Reply::share("http://pan.baidu.com/s/1x?pwd=gk42"),
                Reply::share("https://example.com/s/1x?pwd=gk42"),
                Reply::out(1, "分享失败, 遇到错误\n"),
            ] {
                let (err, subs) = failure("share", reply);
                assert_eq!(err.code(), "upload", "{err}");
                assert!(err.to_string().contains("/gig/a/p-1.zip"), "{err}");
                assert_eq!(subs, ["who", "mkdir", "upload", "meta", "share"]);
            }
        }

        #[test]
        fn the_dir_comes_from_the_remote_root_and_the_key_the_name_from_the_file() {
            for (root, key, dir) in [
                ("/gig", None, "/gig"),
                ("/gig", Some("p-1.zip"), "/gig"),
                ("/gig/", Some("/acme/x/other.zip/"), "/gig/acme/x"),
                ("gig", Some("acme/p-1.zip"), "/gig/acme"),
                ("/", Some("acme/p-1.zip"), "/acme"),
                ("", None, "/"),
            ] {
                let s = setup();
                let mut delivery = Delivery::default();
                delivery.baidupcs.bin = s.fake.bin().to_string_lossy().into_owned();
                delivery.baidupcs.remote_root = root.into();
                let opts = UploadOpts {
                    object_key: key.map(str::to_string),
                    progress: None,
                };
                BaiduPcsUploader::new(&delivery)
                    .upload(&s.local, &opts)
                    .unwrap();
                let calls = s.fake.calls();
                assert_eq!(calls[1], format!("mkdir {dir}"), "{root} {key:?}");
                let file = format!("{}/p-1.zip", dir.trim_end_matches('/'));
                assert_eq!(calls[3], format!("meta {file}"), "{root} {key:?}");
            }
        }

        #[test]
        fn the_share_period_follows_link_ttl_seconds() {
            let s = setup();
            let mut delivery = Delivery::default();
            delivery.baidupcs.bin = s.fake.bin().to_string_lossy().into_owned();
            delivery.link_ttl_seconds = 86_401;
            let before = time::OffsetDateTime::now_utc().unix_timestamp();
            let r = BaiduPcsUploader::new(&delivery)
                .upload(&s.local, &opts("a/p-1.zip"))
                .unwrap();
            assert!(s.fake.calls()[4].starts_with("share set --period 2 -f "));
            let expires = r.expires_at.unwrap();
            assert!((before + 2 * 86_400..=before + 2 * 86_400 + 5).contains(&expires));
        }

        #[test]
        fn who_mkdir_meta_and_share_time_out() {
            for (subcommand, says) in [
                ("who", "baidupcs-go who"),
                ("mkdir", "mkdir /gig/a"),
                ("meta", "meta /gig/a/p-1.zip"),
                ("share", "share /gig/a/p-1.zip"),
            ] {
                let s = setup();
                let uploader = s
                    .uploader
                    .with_call_timeout(std::time::Duration::from_millis(300));
                s.fake.hang(subcommand, 30).unwrap();
                let started = std::time::Instant::now();
                let err = uploader.upload(&s.local, &opts("a/p-1.zip")).unwrap_err();
                assert!(started.elapsed() < std::time::Duration::from_secs(10));
                assert_eq!(err.code(), "upload", "{err}");
                let msg = err.to_string();
                assert!(msg.contains("timed out"), "{msg}");
                assert!(msg.contains(says), "{msg}");
            }
        }

        #[test]
        fn who_without_an_account_is_a_secrets_error_before_any_upload() {
            for reply in [
                Reply::who_logged_out(),
                Reply::out(0, "获取当前帐号失败\n"),
                Reply::out(1, ""),
            ] {
                let s = setup();
                s.fake.reply("who", reply).unwrap();
                let err = s.uploader.upload(&s.local, &opts("a/p-1.zip")).unwrap_err();
                assert_eq!(err.code(), "secrets", "{err}");
                assert!(
                    err.to_string()
                        .contains("! baidupcs-go login -bduss=<BDUSS> -stoken=<STOKEN>"),
                    "{err}"
                );
                assert_eq!(s.fake.calls(), ["who"]);
            }
        }
    }
}
