//! Terminal setup and teardown: raw mode, alternate screen, and a panic hook
//! that restores the terminal before the panic message is printed.
//!
//! The hook is process-wide, but only a panic on the thread that owns the
//! terminal restores it. A panic on the upload worker is caught by the join
//! and shown in a popup while the TUI keeps running, so the hook must leave
//! the terminal alone there; it records the message instead of printing it
//! over the alternate screen (`take_worker_panic`).

use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::sync::{Mutex, Once, OnceLock};
use std::thread::{self, ThreadId};

pub type Term = Terminal<CrosstermBackend<Stdout>>;

static PANIC_HOOK: Once = Once::new();

/// The thread that entered the terminal (the UI thread).
static UI_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Last panic message of a thread other than the UI thread.
static WORKER_PANIC: Mutex<Option<String>> = Mutex::new(None);

/// Raw mode plus alternate screen. Installs the panic hook on first use.
pub fn enter() -> io::Result<Term> {
    install_panic_hook();
    enable_raw_mode()?;
    if let Err(e) = execute!(io::stdout(), EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(e);
    }
    Terminal::new(CrosstermBackend::new(io::stdout())).inspect_err(|_| {
        let _ = leave();
    })
}

/// Undo `enter`. Safe to call more than once.
pub fn leave() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, Show)
}

/// Hand the terminal to a child process ($EDITOR) and take it back.
pub fn suspend_while<T>(term: &mut Term, f: impl FnOnce() -> T) -> io::Result<T> {
    leave()?;
    let out = f();
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    term.clear()?;
    Ok(out)
}

/// Restores the terminal when dropped, covering early returns and errors.
pub struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = leave();
    }
}

/// The message of the last panic on a non-UI thread, if any, cleared.
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
            } else if let Ok(mut slot) = WORKER_PANIC.lock() {
                *slot = Some(info.to_string());
            }
        }));
    });
}
