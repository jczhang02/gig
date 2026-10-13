//! A fake `baidupcs-go` for tests: a `/bin/sh` script that replays canned
//! replies shaped like the live baidupcs-go v4.0.2-dev text output
//! (including the `\r`-separated upload progress). Point
//! `delivery.baidupcs.bin` (or `GIG_DELIVERY_BAIDUPCS_BIN`) at
//! [`FakeBaidupcs::bin`].
//!
//! Available to this crate's tests and, through the `test-util` feature, to
//! other crates' tests. All values are synthetic.
//!
//! The script logs each call's argv to `calls.log` and replies from
//! `<subcommand>.stdout`, `<subcommand>.stderr` and `<subcommand>.exit` in
//! its directory. A subcommand with no reply prints an error and exits 2.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// The size the default replies describe: 50 MiB, as in the live probe.
/// A local file of this size passes the default `meta` check.
pub const SIZE: u64 = 52_428_800;
/// The share link the fake returns by default, without the code.
pub const LINK: &str = "https://pan.baidu.com/s/1gigExampleShareLink";
/// The extraction code the fake returns by default.
pub const PWD: &str = "gk42";
/// The share id the fake returns by default.
pub const SHARE_ID: &str = "13082948151";

/// The line the upload prints before its success line; [`FakeBaidupcs::hold_upload`]
/// stops the reply there.
const SUCCESS_MARK: &str = "[1] 上传文件成功";

const SCRIPT: &str = r#"#!/bin/sh
d=$(dirname "$0")
[ "$1" = "__probe" ] && exit 0
printf '%s\n' "$*" >> "$d/calls.log"
sub=
for a in "$@"; do
  case "$a" in
    -*) ;;
    *) sub=$a; break ;;
  esac
done
if [ -f "$d/$sub.sleep" ]; then exec sleep "$(cat "$d/$sub.sleep")"; fi
if [ ! -f "$d/$sub.exit" ]; then
  echo "fake baidupcs-go has no reply for '$sub'" >&2
  exit 2
fi
[ -f "$d/$sub.stdout" ] && cat "$d/$sub.stdout"
if [ -f "$d/$sub.gate" ]; then
  g=$(cat "$d/$sub.gate")
  n=0
  while [ ! -e "$g" ]; do
    n=$((n + 1))
    if [ "$n" -gt 200 ]; then
      echo "fake baidupcs-go: nobody read the output so far" >&2
      exit 1
    fi
    sleep 0.05
  done
  cat "$d/$sub.after"
fi
[ -f "$d/$sub.stderr" ] && cat "$d/$sub.stderr" >&2
exit "$(cat "$d/$sub.exit")"
"#;

/// One canned reply: exit code, stdout, stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Reply {
    /// Exit `exit` with `stdout` and nothing on stderr.
    pub fn out(exit: i32, stdout: &str) -> Self {
        Self {
            exit,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    /// `who` when logged in.
    pub fn who_ok() -> Self {
        Self::out(
            0,
            "当前帐号 uid: 1000000001, 用户名: gig-test, 性别: 未知, 年龄: 0.0\n",
        )
    }

    /// `who` when no account is logged in: exit 0, uid 0.
    pub fn who_logged_out() -> Self {
        Self::out(0, "当前帐号 uid: 0, 用户名: , 性别: , 年龄: 0.0\n")
    }

    /// `mkdir` that created the directory.
    pub fn mkdir_ok() -> Self {
        Self::out(0, "创建目录成功: /gig/acme\n")
    }

    /// `mkdir` on a directory that exists: exit 0 with a failure text.
    pub fn mkdir_exists() -> Self {
        Self::out(
            0,
            "创建目录 /gig/acme 失败, 遇到错误, 远端服务器返回错误, 代码: 31061, 消息: 文件已存在\n",
        )
    }

    /// The upload of 50 MiB from the live probe: three `\r`-separated
    /// progress lines, then the success line.
    pub fn upload_ok() -> Self {
        Self::out(
            0,
            &upload_transcript(&format!(
                "{SUCCESS_MARK}, 保存到网盘路径: /gig/acme/probe50.zip\n"
            )),
        )
    }

    /// The same name uploaded again: skipped, exit 0, no success line.
    pub fn upload_skipped() -> Self {
        Self::out(
            0,
            &format!(
                "{}{}",
                transcript_head(),
                "[1] 目标文件, /gig/acme/probe50.zip, 已存在, 跳过...\n上传结束, 时间: 1.002s, 总大小: 0B\n"
            ),
        )
    }

    /// The upload without a STOKEN in the login: exit 1.
    pub fn upload_missing_stoken() -> Self {
        Self {
            exit: 1,
            stdout: format!(
                "{}[1] 上传文件失败, 获取用户uk错误, 请确保登录信息包含了STOKEN\n上传结束, 时间: 0.312s, 总大小: 0B\n",
                transcript_head()
            ),
            stderr: "上传失败文件数: 1\n".into(),
        }
    }

    /// `meta` of a file of `size` bytes.
    pub fn meta(size: u64) -> Self {
        Self::out(
            0,
            &format!(
                "\n  类型              文件\n  文件路径          /gig/acme/probe50.zip\n  文件名称          probe50.zip\n  文件大小          {size}, {:.6}MB\n  md5 (截图请打码)  0123456789abcdef0123456789abcdef\n  fs_id             1000000000000001\n",
                size as f64 / 1_048_576.0
            ),
        )
    }

    /// `share set` that shared with `link`; the live reply carries the
    /// code in the link.
    pub fn share(link: &str) -> Self {
        Self::out(0, &format!("shareID: {SHARE_ID}, 链接: {link}\n"))
    }

    /// `share set` with [`LINK`] and [`PWD`].
    pub fn share_ok() -> Self {
        Self::share(&format!("{LINK}?pwd={PWD}"))
    }
}

/// The lines every upload prints before the result.
fn transcript_head() -> String {
    [
        "[0] 提示: 当前上传单个文件最大并发量为: 4, 最大同时上传文件数为: 4",
        "[1] 加入上传队列: /tmp/x/probe50.zip",
        "[1] 准备上传: /tmp/x/probe50.zip",
        "[1] 开始计算文件元信息, 请稍候...",
        "[1] 开始计算文件分块md5, 请稍候...",
        "[1] 开始上传文件...",
        "",
    ]
    .join("\n")
}

/// The full upload output with `result` after the progress.
fn upload_transcript(result: &str) -> String {
    format!(
        "{}\r[1] ↑ 27.50MB/50.00MB 9.61MB/s in 3s ............\r[1] ↑ 43.44MB/50.00MB 5.31MB/s in 6s ............\r[1] ↑ 50.00MB/50.00MB 2.19MB/s in 9s ............\n{result}上传结束, 时间: 11.325s, 总大小: 50.000000MB\n",
        transcript_head()
    )
}

/// A fake `baidupcs-go` installed in a directory the caller owns (a
/// tempdir).
#[derive(Debug, Clone)]
pub struct FakeBaidupcs {
    dir: PathBuf,
}

impl FakeBaidupcs {
    /// Write the script to `<dir>/baidupcs-go`. By default the account is
    /// logged in, and `mkdir`, `upload`, `meta` ([`SIZE`]) and `share` succeed.
    pub fn install(dir: &Path) -> io::Result<Self> {
        let fake = Self {
            dir: dir.to_path_buf(),
        };
        let bin = fake.bin();
        fs::write(&bin, SCRIPT)?;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755))?;
        fake.reply("who", Reply::who_ok())?;
        fake.reply("mkdir", Reply::mkdir_ok())?;
        fake.reply("upload", Reply::upload_ok())?;
        fake.reply("meta", Reply::meta(SIZE))?;
        fake.reply("share", Reply::share_ok())?;
        fake.wait_until_executable(&bin)?;
        Ok(fake)
    }

    /// The script path, for `delivery.baidupcs.bin`.
    pub fn bin(&self) -> PathBuf {
        self.dir.join("baidupcs-go")
    }

    /// Reply to `subcommand` with `reply` from now on.
    pub fn reply(&self, subcommand: &str, reply: Reply) -> io::Result<()> {
        for ext in ["sleep", "gate", "after"] {
            let _ = fs::remove_file(self.file(subcommand, ext));
        }
        fs::write(self.file(subcommand, "stdout"), reply.stdout)?;
        fs::write(self.file(subcommand, "stderr"), reply.stderr)?;
        fs::write(self.file(subcommand, "exit"), reply.exit.to_string())
    }

    /// Make the upload print its progress, then wait (up to 10 s) until
    /// `gate` exists before it prints the success line. A caller that only
    /// reads the output after the exit never sees the progress in time, and
    /// the upload fails.
    pub fn hold_upload(&self, gate: &Path) -> io::Result<()> {
        let stdout = fs::read_to_string(self.file("upload", "stdout"))?;
        let at = stdout
            .find(SUCCESS_MARK)
            .ok_or_else(|| io::Error::other("the upload reply has no success line"))?;
        fs::write(self.file("upload", "stdout"), &stdout[..at])?;
        fs::write(self.file("upload", "after"), &stdout[at..])?;
        fs::write(
            self.file("upload", "gate"),
            gate.as_os_str().as_encoded_bytes(),
        )
    }

    /// Make `subcommand` hang for `seconds` without output.
    pub fn hang(&self, subcommand: &str, seconds: u32) -> io::Result<()> {
        fs::write(self.file(subcommand, "sleep"), seconds.to_string())
    }

    /// Each call's argv after the binary, space-joined, oldest first.
    pub fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn file(&self, subcommand: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{subcommand}.{ext}"))
    }

    /// Another test thread may fork while the script was open for writing;
    /// until that child execs, running the script fails with ETXTBSY.
    fn wait_until_executable(&self, bin: &Path) -> io::Result<()> {
        const ETXTBSY: i32 = 26;
        let mut tries = 0;
        loop {
            let probe = Command::new(bin)
                .arg("__probe")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            match probe {
                Err(e) if e.raw_os_error() == Some(ETXTBSY) && tries < 50 => {
                    tries += 1;
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => return Err(e),
                Ok(_) => return Ok(()),
            }
        }
    }
}
