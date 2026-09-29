//! Application state, global key routing (spec section 2), and the event loop.

use crate::data::Snapshot;
use crate::icons::Icons;
use crate::terminal::Term;
use crate::theme::Theme;
use crate::ui;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use gig_core::config::Tui;
use gig_core::services::Ctx;
use gig_core::Result;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Orders,
    Drafts,
    Money,
    History,
}

impl View {
    pub const ALL: [View; 4] = [View::Orders, View::Drafts, View::Money, View::History];

    pub fn index(self) -> usize {
        match self {
            View::Orders => 0,
            View::Drafts => 1,
            View::Money => 2,
            View::History => 3,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            View::Orders => "Orders",
            View::Drafts => "Drafts",
            View::Money => "Money",
            View::History => "History",
        }
    }

    /// `1`..`4`.
    pub fn from_digit(c: char) -> Option<View> {
        let n = c.to_digit(10)? as usize;
        n.checked_sub(1).and_then(|i| View::ALL.get(i).copied())
    }

    pub fn next(self) -> View {
        View::ALL[(self.index() + 1) % View::ALL.len()]
    }

    pub fn prev(self) -> View {
        View::ALL[(self.index() + View::ALL.len() - 1) % View::ALL.len()]
    }
}

/// Per-view list filter typed after `/`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub text: String,
    /// True while the hint line is taking input.
    pub editing: bool,
}

/// Everything the renderer needs. Kept apart from `Ctx` so drawing can be
/// tested without a database.
#[derive(Debug, Clone, PartialEq)]
pub struct UiState {
    pub view: View,
    pub filters: [Filter; 4],
    pub help_open: bool,
    /// Full-screen detail (narrow terminals, and History `Enter`).
    pub detail_open: bool,
    /// Last refresh error, shown on the hint line until the next success.
    pub error: Option<String>,
    pub data: Snapshot,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            view: View::Orders,
            filters: Default::default(),
            help_open: false,
            detail_open: false,
            error: None,
            data: Snapshot::default(),
        }
    }
}

/// What a key press asks the loop to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    None,
    Quit,
    Refresh,
    /// Not a global key: the current view gets it (later tickets).
    Unhandled(KeyEvent),
}

/// Terminal width from which the detail pane sits next to the Orders list.
pub const WIDE_COLUMNS: u16 = 110;

impl UiState {
    pub fn filter(&self) -> &Filter {
        &self.filters[self.view.index()]
    }

    fn filter_mut(&mut self) -> &mut Filter {
        &mut self.filters[self.view.index()]
    }

    fn switch(&mut self, view: View) {
        if self.view != view {
            self.filter_mut().editing = false;
            self.view = view;
            self.detail_open = false;
        }
    }

    /// Route one key press. Precedence: help popup, filter input, global keys.
    /// `width` is the terminal width, for the adaptive `Enter`.
    pub fn handle_key(&mut self, key: KeyEvent, width: u16) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Outcome::Quit;
        }
        if self.help_open {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => self.help_open = false,
                KeyCode::Char('q') => return Outcome::Quit,
                _ => {}
            }
            return Outcome::None;
        }
        if self.filter().editing {
            let f = self.filter_mut();
            match key.code {
                KeyCode::Esc => *f = Filter::default(),
                KeyCode::Enter => f.editing = false,
                KeyCode::Backspace => {
                    f.text.pop();
                }
                KeyCode::Char(c) => f.text.push(c),
                _ => {}
            }
            return Outcome::None;
        }
        match key.code {
            KeyCode::Char('q') => Outcome::Quit,
            KeyCode::Char('?') => {
                self.help_open = true;
                Outcome::None
            }
            KeyCode::Char('r') => Outcome::Refresh,
            KeyCode::Char(c @ '1'..='4') => {
                if let Some(v) = View::from_digit(c) {
                    self.switch(v);
                }
                Outcome::None
            }
            KeyCode::Tab => {
                self.switch(self.view.next());
                Outcome::None
            }
            KeyCode::BackTab => {
                self.switch(self.view.prev());
                Outcome::None
            }
            KeyCode::Char('/') => {
                self.filter_mut().editing = true;
                Outcome::None
            }
            KeyCode::Esc => {
                if self.detail_open {
                    self.detail_open = false;
                } else {
                    *self.filter_mut() = Filter::default();
                }
                Outcome::None
            }
            KeyCode::Enter
                if (self.view == View::Orders && width < WIDE_COLUMNS)
                    || self.view == View::History =>
            {
                self.detail_open = true;
                Outcome::None
            }
            _ => Outcome::Unhandled(key),
        }
    }
}

/// The running dashboard: the one database handle plus the UI state.
pub struct App {
    pub ctx: Ctx,
    pub ui: UiState,
    pub theme: Theme,
    pub icons: Icons,
    /// `None` when `refresh_seconds` is 0.
    pub refresh_every: Option<Duration>,
}

/// How long to wait for input when the refresh timer is off.
const IDLE_POLL: Duration = Duration::from_secs(60);

impl App {
    pub fn new(ctx: Ctx, settings: &Tui) -> Self {
        Self {
            ctx,
            ui: UiState::default(),
            theme: Theme::new(settings.light),
            icons: Icons::new(settings.icons),
            refresh_every: (settings.refresh_seconds > 0)
                .then(|| Duration::from_secs(settings.refresh_seconds)),
        }
    }

    /// Reload the snapshot. Errors land on the hint line; nothing is retried.
    pub fn refresh(&mut self) {
        match Snapshot::load(&self.ctx) {
            Ok(data) => {
                self.ui.data = data;
                self.ui.error = None;
            }
            Err(e) => self.ui.error = Some(format!("{}: {e}", e.code())),
        }
    }

    pub fn run_loop(&mut self, term: &mut Term) -> Result<()> {
        let mut next_tick = self.refresh_every.map(|d| Instant::now() + d);
        loop {
            term.draw(|frame| {
                let cx = ui::RenderCx {
                    state: &self.ui,
                    theme: &self.theme,
                    icons: &self.icons,
                };
                ui::draw(frame, &cx);
            })?;

            let timeout = match next_tick {
                Some(t) => t.saturating_duration_since(Instant::now()),
                None => IDLE_POLL,
            };
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    let width = term.size()?.width;
                    match self.ui.handle_key(key, width) {
                        Outcome::Quit => return Ok(()),
                        Outcome::Refresh => self.refresh(),
                        Outcome::None | Outcome::Unhandled(_) => {}
                    }
                }
                // Resize and other events just redraw.
            }
            if let (Some(t), Some(every)) = (next_tick, self.refresh_every) {
                if Instant::now() >= t {
                    self.refresh();
                    next_tick = Some(Instant::now() + every);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(state: &mut UiState, code: KeyCode) -> Outcome {
        state.handle_key(KeyEvent::new(code, KeyModifiers::NONE), 200)
    }

    #[test]
    fn digits_and_tab_switch_views() {
        let mut s = UiState::default();
        assert_eq!(s.view, View::Orders);
        press(&mut s, KeyCode::Char('3'));
        assert_eq!(s.view, View::Money);
        press(&mut s, KeyCode::Tab);
        assert_eq!(s.view, View::History);
        press(&mut s, KeyCode::Tab);
        assert_eq!(s.view, View::Orders);
        press(&mut s, KeyCode::BackTab);
        assert_eq!(s.view, View::History);
        assert_eq!(View::from_digit('5'), None);
        assert_eq!(View::from_digit('0'), None);
    }

    #[test]
    fn quit_refresh_and_help() {
        let mut s = UiState::default();
        assert_eq!(press(&mut s, KeyCode::Char('q')), Outcome::Quit);
        assert_eq!(press(&mut s, KeyCode::Char('r')), Outcome::Refresh);
        press(&mut s, KeyCode::Char('?'));
        assert!(s.help_open);
        // Global keys are swallowed while help is open.
        assert_eq!(press(&mut s, KeyCode::Char('2')), Outcome::None);
        assert_eq!(s.view, View::Orders);
        press(&mut s, KeyCode::Esc);
        assert!(!s.help_open);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(s.handle_key(ctrl_c, 80), Outcome::Quit);
    }

    #[test]
    fn filter_input_and_esc() {
        let mut s = UiState::default();
        press(&mut s, KeyCode::Char('/'));
        assert!(s.filter().editing);
        for c in "q1x".chars() {
            // Typed into the filter, not treated as quit or view keys.
            assert_eq!(press(&mut s, KeyCode::Char(c)), Outcome::None);
        }
        press(&mut s, KeyCode::Backspace);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.filter().text, "q1");
        assert!(!s.filter().editing);
        assert_eq!(s.view, View::Orders);
        // Filters are per view.
        press(&mut s, KeyCode::Char('2'));
        assert_eq!(s.filter().text, "");
        press(&mut s, KeyCode::Char('1'));
        assert_eq!(s.filter().text, "q1");
        press(&mut s, KeyCode::Esc);
        assert_eq!(s.filter(), &Filter::default());
    }

    #[test]
    fn enter_opens_detail_only_when_narrow() {
        let mut s = UiState::default();
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert!(matches!(s.handle_key(enter, 200), Outcome::Unhandled(_)));
        assert!(!s.detail_open);
        s.handle_key(enter, 80);
        assert!(s.detail_open);
        press(&mut s, KeyCode::Esc);
        assert!(!s.detail_open);
        press(&mut s, KeyCode::Char('4'));
        s.handle_key(enter, 200);
        assert!(s.detail_open);
    }

    #[test]
    fn view_keys_pass_through() {
        let mut s = UiState::default();
        assert!(matches!(
            press(&mut s, KeyCode::Char('s')),
            Outcome::Unhandled(_)
        ));
    }
}
