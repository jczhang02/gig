//! Running an uploader's CLI as a subprocess: finding the binary, spawning
//! it, waiting with an optional timeout, and redacting token-like values
//! from what it printed. Shared by the CLI-driven uploaders.

use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// True when `bin` names an executable file: a path as given, or a bare
/// name found on PATH (what running it would find).
pub fn bin_resolves(bin: &str) -> bool {
    find_bin(bin, std::env::var_os("PATH")).is_some()
}

/// The executable `bin` names: a path with a separator as given, a bare
/// name in the first directory of `path` that holds it.
pub(crate) fn find_bin(bin: &str, path: Option<OsString>) -> Option<PathBuf> {
    if bin.is_empty() {
        return None;
    }
    let given = Path::new(bin);
    if given.components().count() > 1 {
        return is_executable(given).then(|| given.to_path_buf());
    }
    std::env::split_paths(&path?)
        .map(|dir| dir.join(bin))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// Why a call did not give a usable reply. Messages are redacted.
pub(crate) enum CallError {
    NotFound,
    NotExecutable,
    TimedOut(Duration),
    Failed(String),
}

impl CallError {
    pub(crate) fn message(&self) -> String {
        match self {
            CallError::NotFound => "binary not found".into(),
            CallError::NotExecutable => "binary not executable".into(),
            CallError::TimedOut(t) => format!("timed out after {} s", t.as_secs()),
            CallError::Failed(m) => m.clone(),
        }
    }
}

/// What a finished call printed, unredacted.
pub(crate) struct Output {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Start `bin args` with stdin closed and stdout and stderr piped.
pub(crate) fn spawn(bin: &str, args: &[&str]) -> Result<Child, CallError> {
    Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => CallError::NotFound,
            io::ErrorKind::PermissionDenied => CallError::NotExecutable,
            _ => CallError::Failed(format!("cannot run it: {e}")),
        })
}

/// Run `bin args` to the end (or until `timeout`, then kill it) and collect
/// what it printed.
pub(crate) fn run(
    bin: &str,
    args: &[&str],
    timeout: Option<Duration>,
) -> Result<Output, CallError> {
    let mut child = spawn(bin, args)?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = wait(&mut child, timeout)?;
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// Read `pipe` to the end on its own thread.
pub(crate) fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// Wait for `child`; with a timeout, kill it when the timeout passes.
pub(crate) fn wait(child: &mut Child, timeout: Option<Duration>) -> Result<ExitStatus, CallError> {
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

/// Blank token-like values, as `redact()` in the verification script does:
/// `(access_token|refresh_token|authorization|bduss|stoken)[=":\s]+` then the
/// value up to whitespace or one of `&,"<>`.
pub(crate) fn redact(text: &str) -> String {
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

    #[cfg(unix)]
    #[test]
    fn the_bin_is_found_as_a_path_or_on_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("baidupcs-go");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let plain = dir.path().join("plain");
        std::fs::write(&plain, "").unwrap();
        let path =
            Some(std::env::join_paths(["/nonexistent-gig-dir".as_ref(), dir.path()]).unwrap());

        assert_eq!(find_bin("baidupcs-go", path.clone()), Some(exe.clone()));
        assert_eq!(find_bin(exe.to_str().unwrap(), None), Some(exe.clone()));
        // Not executable, missing, empty, or no PATH at all: not found.
        assert_eq!(find_bin("plain", path.clone()), None);
        assert_eq!(find_bin(plain.to_str().unwrap(), path.clone()), None);
        assert_eq!(find_bin("nope", path.clone()), None);
        assert_eq!(find_bin("", path), None);
        assert_eq!(find_bin("baidupcs-go", None), None);
        // A directory of that name is not the binary.
        assert_eq!(find_bin(dir.path().to_str().unwrap(), None), None);
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
}
