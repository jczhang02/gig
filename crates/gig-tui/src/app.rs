//! Application state, global key routing (spec section 2), and the event loop.

use crate::actions::{self, Action, Effect};
use crate::data::{OrderRow, Snapshot};
use crate::icons::Icons;
use crate::popup::Popup;
use crate::terminal::{self, Term};
use crate::theme::Theme;
use crate::ui;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use gig_core::config::Tui;
use gig_core::models::Draft;
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
    /// Open action popup (form, confirmation, result, refusal). Takes every
    /// key while open.
    pub popup: Option<Popup>,
    /// Selected order id in the Orders and History lists. An id rather than a
    /// row index so the selection survives re-sorting on refresh; `None` or a
    /// vanished id means the first row.
    pub selected: Option<i64>,
    /// Selected draft id in the Drafts view, same rules as `selected`.
    pub selected_draft: Option<i64>,
    /// `a` in Orders: include archived and cancelled orders.
    pub show_closed: bool,
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
            popup: None,
            selected: None,
            selected_draft: None,
            show_closed: false,
        }
    }
}

/// What a key press asks the loop to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    None,
    Quit,
    /// Reload the snapshot. Returned by `r`, and by every action handler once
    /// its gig-core call has returned (success or refusal), so the lists
    /// show the new state (spec section 4).
    Refresh,
    /// An action key or popup key: carry out the effect (`App::apply`).
    Act(Effect),
    /// Not a key of this app state: the current view may use it.
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

    /// The order list the selection moves in: History in History, else the
    /// Orders list (with closed orders when `show_closed`).
    pub fn order_list(&self) -> Vec<&OrderRow> {
        if self.view == View::History {
            self.data.history()
        } else {
            self.data.active_orders(self.show_closed)
        }
    }

    /// The order actions apply to: the selected one, else the first row.
    pub fn selected_order(&self) -> Option<&OrderRow> {
        let list = self.order_list();
        self.selected
            .and_then(|id| list.iter().find(|r| r.order.id == id))
            .or(list.first())
            .copied()
    }

    /// The open draft `P` promotes: the selected one, else the first.
    pub fn selected_draft(&self) -> Option<&Draft> {
        let mut open = self.data.open_drafts();
        let first = self.data.open_drafts().next();
        self.selected_draft
            .and_then(|id| open.find(|d| d.id == id))
            .or(first)
    }

    /// Up/Down in the current list, clamped at both ends.
    fn move_selection(&mut self, delta: isize) {
        fn step(ids: &[i64], current: Option<i64>, delta: isize) -> Option<i64> {
            let at = current
                .and_then(|id| ids.iter().position(|&x| x == id))
                .unwrap_or(0) as isize;
            let last = ids.len().checked_sub(1)? as isize;
            ids.get((at + delta).clamp(0, last) as usize).copied()
        }
        match self.view {
            View::Orders | View::History => {
                let ids: Vec<i64> = self.order_list().iter().map(|r| r.order.id).collect();
                let cur = self.selected_order().map(|r| r.order.id);
                self.selected = step(&ids, cur, delta);
            }
            View::Drafts => {
                let ids: Vec<i64> = self.data.open_drafts().map(|d| d.id).collect();
                let cur = self.selected_draft().map(|d| d.id);
                self.selected_draft = step(&ids, cur, delta);
            }
            View::Money => {}
        }
    }

    /// Route one key press. Precedence: action popup, help popup, filter
    /// input, global keys, list and action keys.
    /// `width` is the terminal width, for the adaptive `Enter`.
    pub fn handle_key(&mut self, key: KeyEvent, width: u16) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Outcome::Quit;
        }
        if self.popup.is_some() {
            return Outcome::Act(actions::popup_key(self, key));
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
            KeyCode::Up => {
                self.move_selection(-1);
                Outcome::None
            }
            KeyCode::Down => {
                self.move_selection(1);
                Outcome::None
            }
            KeyCode::Char('a') if self.view == View::Orders && !self.detail_open => {
                self.show_closed = !self.show_closed;
                Outcome::None
            }
            _ => match actions::view_key(self, key) {
                Some(effect) => Outcome::Act(effect),
                None => Outcome::Unhandled(key),
            },
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
    /// Created on first copy and kept: on X11 the clipboard text lives only
    /// as long as this handle. `Err` remembers that there is no clipboard.
    clipboard: Option<std::result::Result<arboard::Clipboard, String>>,
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
            clipboard: None,
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
                        Outcome::Act(effect) => self.apply(effect, term)?,
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

    /// Carry out an action effect. gig-core calls end with a refresh whether
    /// they succeeded or were refused (spec section 4).
    pub fn apply(&mut self, effect: Effect, term: &mut Term) -> Result<()> {
        match effect {
            Effect::None => {}
            Effect::Refresh => self.refresh(),
            Effect::Call(action) => self.call(&action),
            Effect::EditField(index) => {
                let initial = actions::field_text(&self.ui, index);
                match terminal::suspend_while(term, || crate::editor::edit_text(&initial))? {
                    Ok(text) => actions::set_edited_field(&mut self.ui, index, text),
                    Err(e) => self.editor_failed(e),
                }
            }
            Effect::EditNote { slug } => {
                match terminal::suspend_while(term, || crate::editor::edit_text(""))? {
                    // An empty note means "changed my mind".
                    Ok(text) if text.trim().is_empty() => {}
                    Ok(text) => self.call(&Action::Note { slug, text }),
                    Err(e) => self.editor_failed(e),
                }
            }
            Effect::EditFile(path) => {
                if let Err(e) = terminal::suspend_while(term, || crate::editor::edit_file(&path))? {
                    self.editor_failed(e);
                }
                self.refresh();
            }
            Effect::Copy(link) => self.copy(link),
        }
        Ok(())
    }

    fn call(&mut self, action: &Action) {
        actions::perform(&self.ctx, &mut self.ui, action);
        self.refresh();
    }

    fn editor_failed(&mut self, e: std::io::Error) {
        self.ui.popup = Some(Popup::Message {
            title: "editor".into(),
            lines: vec![format!("{}: {e}", crate::editor::command().join(" "))],
            error: true,
        });
    }

    /// `y`: the link goes to the clipboard; without one it is only shown.
    fn copy(&mut self, link: String) {
        let clip = self
            .clipboard
            .get_or_insert_with(|| arboard::Clipboard::new().map_err(|e| e.to_string()));
        let lines = match clip {
            Ok(c) => match c.set_text(link.clone()) {
                Ok(()) => vec!["copied to the clipboard:".into(), link],
                Err(e) => vec![format!("clipboard unavailable ({e}):"), link],
            },
            Err(e) => vec![format!("clipboard unavailable ({e}):"), link],
        };
        self.ui.popup = Some(Popup::message("link", lines));
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
