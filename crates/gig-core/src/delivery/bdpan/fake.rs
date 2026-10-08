//! A fake `bdpan` for tests: a `/bin/sh` script that replays canned replies
//! shaped like the live bdpan 3.8.7 JSON. Point `delivery.bdpan.bin` (or
//! `GIG_DELIVERY_BDPAN_BIN`) at [`FakeBdpan::bin`].
//!
//! Available to this crate's tests and, through the `test-util` feature, to
//! other crates' tests. All values are synthetic.
//!
//! The script logs each call's argv to `calls.log` and replies from
//! `<subcommand>.stdout`, `<subcommand>.stderr` and `<subcommand>.exit` in
//! its directory. A subcommand with no reply prints an `Error:` line and
//! exits 2.

use serde_json::json;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// The share link the fake returns by default.
pub const LINK: &str = "https://pan.baidu.com/s/1gigExampleShareLink";
/// The extraction code the fake returns by default.
pub const PWD: &str = "gk42";

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
  echo "Error: fake bdpan has no reply for '$sub'" >&2
  exit 2
fi
[ -f "$d/$sub.stdout" ] && cat "$d/$sub.stdout"
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
    /// Exit 0 with `value` as pretty JSON on stdout, as `bdpan --json` does.
    pub fn json(value: serde_json::Value) -> Self {
        Self {
            exit: 0,
            stdout: format!("{value:#}\n"),
            stderr: String::new(),
        }
    }

    /// A non-zero exit with nothing on stdout and `stderr` as given.
    pub fn fail(exit: i32, stderr: &str) -> Self {
        Self {
            exit,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }

    /// `whoami --json`.
    pub fn whoami(authenticated: bool, has_valid_token: bool, expires_at: &str) -> Self {
        Self::json(json!({
            "authenticated": authenticated,
            "username": "gig-test",
            "expires_at": expires_at,
            "has_valid_token": has_valid_token,
            "token_expires_in": "29 天 23 小时",
        }))
    }

    /// A successful `upload --json`.
    pub fn upload_ok() -> Self {
        Self::json(json!({
            "code": 0,
            "data": {
                "agent_reply": "上传成功: 我的应用数据/bdpan/gig/example.zip",
                "fsid": "1000000000000001",
                "remote": "gig/example.zip",
                "remote_path": "/apps/bdpan/gig/example.zip",
                "return_hint": "点击查看",
            },
            "error": "",
        }))
    }

    /// The live business failure: exit 0, `code` 1, an `error` message.
    pub fn upload_business_error() -> Self {
        Self::json(json!({
            "code": 1,
            "data": null,
            "error": "本地路径不存在: /nope/example.zip",
        }))
    }

    /// A successful `share --json` with [`LINK`] and [`PWD`].
    pub fn share_ok(period: u32) -> Self {
        Self::json(json!({
            "link": LINK,
            "short_url": "gigExampleShareLink",
            "share_id": 1,
            "period": period,
            "pwd": PWD,
        }))
    }

    /// The live share failure: exit 1, empty stdout, an `Error:` line then usage.
    pub fn share_missing_remote() -> Self {
        Self::fail(
            1,
            "Error: 文件或文件夹不存在: gig/example.zip\nUsage:\n  bdpan share <path> [flags]\n",
        )
    }
}

/// A fake `bdpan` installed in a directory the caller owns (a tempdir).
#[derive(Debug, Clone)]
pub struct FakeBdpan {
    dir: PathBuf,
}

impl FakeBdpan {
    /// Write the script to `<dir>/bdpan`. By default the account is logged
    /// in, and `upload` and `share` (7 days) succeed.
    pub fn install(dir: &Path) -> io::Result<Self> {
        let fake = Self {
            dir: dir.to_path_buf(),
        };
        let bin = fake.bin();
        fs::write(&bin, SCRIPT)?;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755))?;
        fake.reply("whoami", Reply::whoami(true, true, "2099-01-01T00:00:00Z"))?;
        fake.reply("upload", Reply::upload_ok())?;
        fake.reply("share", Reply::share_ok(7))?;
        fake.wait_until_executable(&bin)?;
        Ok(fake)
    }

    /// The script path, for `delivery.bdpan.bin`.
    pub fn bin(&self) -> PathBuf {
        self.dir.join("bdpan")
    }

    /// Reply to `subcommand` with `reply` from now on.
    pub fn reply(&self, subcommand: &str, reply: Reply) -> io::Result<()> {
        let _ = fs::remove_file(self.dir.join(format!("{subcommand}.sleep")));
        fs::write(self.file(subcommand, "stdout"), reply.stdout)?;
        fs::write(self.file(subcommand, "stderr"), reply.stderr)?;
        fs::write(self.file(subcommand, "exit"), reply.exit.to_string())
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
