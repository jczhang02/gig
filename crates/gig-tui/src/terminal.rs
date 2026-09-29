//! Terminal setup and teardown: raw mode, alternate screen, and a panic hook
//! that restores the terminal before the panic message is printed.
//!
//! The hook is process-wide, but only a panic on the thread that owns the
//! terminal restores it. A panic on the upload worker is caught by the join
//! and shown in a popup while the TUI keeps running, so the hook must leave
//! the terminal alone there; it records the message instead of printing it
//! over the alternate screen (`take_worker_panic`).
//!
//! Mouse capture (TUI-SPEC 8.2) is on while the dashboard owns the terminal
//! and `tui.mouse` (or `M`) says so. Every way out (quit, error, panic,
//! `$EDITOR`) goes through `leave`, which always turns it off, and
//! `suspend_while` turns it back on afterwards.
//!
//! Capture asks only for clicks, drags and the wheel (`?1000`, `?1002`,
//! SGR `?1006`), not for every pointer motion (`?1003`, which crossterm's
//! `EnableMouseCapture` turns on). The app ignores motion, and a sweep of
//! the pointer would otherwise queue kilobytes of reports: crossterm 0.29
//! reads the tty in 1024-byte chunks and can split a report, and a split
//! report swallows the next reply or key. For the same reason `leave`
//! throws away input still queued, so reports never reach the shell or
//! `$EDITOR`, and redraws never ask the terminal for the cursor position.

use crossterm::cursor::Show;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, Command};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use std::fmt;
use std::io::{self, Stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::thread::{self, ThreadId};
use std::time::Duration;

pub type Term = Terminal<CrosstermBackend<Stdout>>;

static PANIC_HOOK: Once = Once::new();

/// The thread that entered the terminal (the UI thread).
static UI_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Name of the upload worker thread, the only thread whose panics are
/// recorded (arboard runs its own threads, whose panics must not be
/// reported as an upload failure).
pub const WORKER_THREAD: &str = "gig-upload";

/// Last panic message of the upload worker.
static WORKER_PANIC: Mutex<Option<String>> = Mutex::new(None);

/// The dashboard holds the terminal (between `enter` and `leave`).
static ENTERED: AtomicBool = AtomicBool::new(false);

/// Mouse capture is wanted: set by `set_mouse`, kept across `$EDITOR`.
static MOUSE: AtomicBool = AtomicBool::new(false);

/// Raw mode plus alternate screen. Installs the panic hook on first use.
pub fn enter() -> io::Result<Term> {
    install_panic_hook();
    enable_raw_mode()?;
    // Bracketed paste: a paste arrives as one event, not as keystrokes that
    // would run actions (`s`, `x`) or submit a form on a newline.
    if let Err(e) = execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste) {
        let _ = disable_raw_mode();
        return Err(e);
    }
    ENTERED.store(true, Ordering::SeqCst);
    Terminal::new(CrosstermBackend::new(io::stdout())).inspect_err(|_| {
        let _ = leave();
    })
}

/// Clicks, drags and the wheel in SGR encoding; no motion reports.
struct EnableMouse;

impl Command for EnableMouse {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1b[?1000h\x1b[?1002h\x1b[?1006h")
    }
    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        crossterm::event::EnableMouseCapture.execute_winapi()
    }
}

/// Every mouse mode off, `?1003` included (harmless when it was not on).
struct DisableMouse;

impl Command for DisableMouse {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l")
    }
    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        crossterm::event::DisableMouseCapture.execute_winapi()
    }
}

/// Undo `enter`. Safe to call more than once. Mouse capture is always
/// turned off (harmless when it was not on), so the shell gets its mouse
/// back after a quit, an error or a panic. The escape codes are sent even
/// when raw mode cannot be left, and input still queued (mouse reports,
/// keys typed ahead) is thrown away so it never reaches the next reader of
/// the terminal. The first error is returned.
pub fn leave() -> io::Result<()> {
    let was_entered = ENTERED.swap(false, Ordering::SeqCst);
    let out = execute!(
        io::stdout(),
        DisableMouse,
        DisableBracketedPaste,
        LeaveAlternateScreen,
        Show
    );
    if was_entered {
        discard_input();
    }
    let raw = disable_raw_mode();
    out.and(raw)
}

/// Drop what is queued on stdin: crossterm's own buffer, then the tty's.
/// The terminal was just told to stop reporting the mouse; reports already
/// on their way are read until the input has been quiet for a moment, and
/// bytes crossterm left in the tty are made readable with `nudge`, so a
/// report it had only half read is completed and dropped rather than left
/// in its parser, where it would eat the first keys typed after `$EDITOR`.
fn discard_input() {
    let started = std::time::Instant::now();
    for _ in 0..64 {
        if started.elapsed() > DRAIN_MAX {
            break;
        }
        // What crossterm has already parsed.
        for _ in 0..4096 {
            match crossterm::event::poll(Duration::ZERO) {
                Ok(true) if crossterm::event::read().is_ok() => {}
                _ => break,
            }
        }
        let quiet = if ask_status_if_pending() {
            DRAIN_REPLY
        } else {
            DRAIN_QUIET
        };
        if !matches!(crossterm::event::poll(quiet), Ok(true)) {
            break;
        }
    }
    #[cfg(unix)]
    // SAFETY: tcflush only reads its two integer arguments; on a stdin that
    // is not a terminal it fails with ENOTTY, which is ignored.
    unsafe {
        libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH);
    }
}

/// Input quiet this long counts as drained.
const DRAIN_QUIET: Duration = Duration::from_millis(30);
/// How long to wait for the terminal's reply to a nudge.
const DRAIN_REPLY: Duration = Duration::from_millis(100);
/// Longest the drain may take, however much keeps arriving.
const DRAIN_MAX: Duration = Duration::from_millis(500);

/// Bytes the tty holds that nobody has read yet.
#[cfg(unix)]
fn pending_input() -> usize {
    let mut n: libc::c_int = 0;
    // SAFETY: FIONREAD writes one c_int through the pointer, which points
    // at a live local.
    let ok = unsafe { libc::ioctl(libc::STDIN_FILENO, libc::FIONREAD, &mut n) } == 0;
    if ok {
        usize::try_from(n).unwrap_or(0)
    } else {
        0
    }
}

/// crossterm 0.29 reads the tty only when new input arrives (its epoll is
/// edge-triggered) and stops after the first 1024-byte chunk that holds an
/// event, so the rest of a larger burst waits in the tty until the next key
/// arrives. When bytes are waiting there, ask the terminal where its cursor
/// is (`CSI 6 n`): the reply is new input, so crossterm reads again. The
/// reply must parse to something: crossterm's read loop only returns once a
/// chunk yields an event, and the tty is a blocking fd, so a reply that
/// parses to nothing (`CSI 5 n`'s) would leave it blocked in `read` until
/// the next key. A cursor report is an internal crossterm event that
/// `event::read` never returns. Returns whether it asked.
pub fn nudge() -> bool {
    ENTERED.load(Ordering::SeqCst) && ask_status_if_pending()
}

/// `nudge` without the check that the dashboard holds the terminal (used
/// by `leave` while it drains).
fn ask_status_if_pending() -> bool {
    #[cfg(unix)]
    if pending_input() > 0 {
        let mut out = io::stdout();
        return out.write_all(b"\x1b[6n").and_then(|_| out.flush()).is_ok();
    }
    false
}

/// Clear the screen and make the next draw repaint every cell, without
/// `Terminal::clear`, which asks the terminal for the cursor position: a
/// reply that arrives inside a split mouse report is never recognised and
/// the dashboard would wait for it.
pub fn full_clear(term: &mut Term) -> io::Result<()> {
    let size = term.size()?;
    term.resize(Rect::new(0, 0, size.width, size.height))?;
    io::stdout().flush()
}

/// Turn mouse capture on or off. Outside the dashboard (tests, before
/// `enter`) only the wish is recorded, so no escape codes reach a pipe.
pub fn set_mouse(on: bool) -> io::Result<()> {
    MOUSE.store(on, Ordering::SeqCst);
    if !ENTERED.load(Ordering::SeqCst) {
        return Ok(());
    }
    if on {
        execute!(io::stdout(), EnableMouse)
    } else {
        execute!(io::stdout(), DisableMouse)
    }
}

/// True while mouse capture is wanted.
pub fn mouse() -> bool {
    MOUSE.load(Ordering::SeqCst)
}

/// Hand the terminal to a child process ($EDITOR) and take it back; the
/// editor gets the mouse too, and capture comes back with the dashboard.
pub fn suspend_while<T>(term: &mut Term, f: impl FnOnce() -> T) -> io::Result<T> {
    leave()?;
    let out = f();
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    ENTERED.store(true, Ordering::SeqCst);
    if mouse() {
        execute!(io::stdout(), EnableMouse)?;
    }
    full_clear(term)?;
    Ok(out)
}

/// Restores the terminal when dropped, covering early returns and errors.
pub struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = leave();
    }
}

/// The message of the last upload worker panic, if any, cleared.
pub fn take_worker_panic() -> Option<String> {
    WORKER_PANIC.lock().ok()?.take()
}

fn install_panic_hook() {
    let _ = UI_THREAD.set(thread::current().id());
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let on_ui_thread = UI_THREAD.get() == Some(&thread::current().id());
            if on_ui_thread {
                let _ = leave();
                previous(info);
            } else if thread::current().name() == Some(WORKER_THREAD) {
                if let Ok(mut slot) = WORKER_PANIC.lock() {
                    *slot = Some(info.to_string());
                }
            }
        }));
    });
}
