//! Terminal setup and teardown: raw mode, alternate screen, and a panic hook
//! that restores the terminal before the panic message is printed.

use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::sync::Once;

pub type Term = Terminal<CrosstermBackend<Stdout>>;

static PANIC_HOOK: Once = Once::new();

/// Raw mode plus alternate screen. Installs the panic hook on first use.
pub fn enter() -> io::Result<Term> {
    install_panic_hook();
    enable_raw_mode()?;
    if let Err(e) = execute!(io::stdout(), EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(e);
    }
    Terminal::new(CrosstermBackend::new(io::stdout()))
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

fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = leave();
            previous(info);
        }));
    });
}
