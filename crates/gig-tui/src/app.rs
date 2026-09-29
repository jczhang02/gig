//! Application state, global key routing (spec section 2), and the event loop.

use crate::actions::{self, Action, Effect};
use crate::data::{JobCache, OrderRow, Snapshot};
use crate::icons::Icons;
use crate::popup::{Form, Popup, Scroll};
use crate::terminal::{self, Term};
use crate::theme::Theme;
use crate::ui;
use crate::upload::{self, UploadJob};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use gig_core::config::Tui;
use gig_core::delivery::configured_uploader;
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

/// Case-insensitive substring match of `filter` in any of `fields`; an
/// empty filter matches everything.
pub fn matches_filter(filter: &str, fields: &[&str]) -> bool {
    let needle = filter.trim().to_lowercase();
    needle.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&needle))
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
    /// Scroll of the order detail (pane or full screen): PgUp/PgDn/Home/End.
    /// Back to the top whenever the selection or the view changes.
    pub detail_scroll: Scroll,
    /// The last submitted form while its call is pending or was refused:
    /// closing the refusal brings it back with everything typed.
    pub last_form: Option<Form>,
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
            detail_scroll: Scroll::default(),
            last_form: None,
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
    /// Ctrl+L: clear the terminal and draw everything again.
    Redraw,
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
            self.detail_scroll.offset = 0;
        }
    }

    /// The order list the selection moves in: History in History, else the
    /// Orders list (with closed orders when `show_closed`).
    /// The views draw exactly this list, filtered by the view's `/` text, so
    /// actions always target a visible row.
    pub fn order_list(&self) -> Vec<&OrderRow> {
        let list = if self.view == View::History {
            self.data.history()
        } else {
            self.data.active_orders(self.show_closed)
        };
        let f = &self.filter().text;
        list.into_iter()
            .filter(|r| {
                let o = &r.order;
                matches_filter(f, &[&o.slug, &o.title, o.platform.as_deref().unwrap_or("")])
            })
            .collect()
    }

    /// The order actions apply to: the selected one, else the first row.
    pub fn selected_order(&self) -> Option<&OrderRow> {
        let list = self.order_list();
        self.selected
            .and_then(|id| list.iter().find(|r| r.order.id == id))
            .or(list.first())
            .copied()
    }

    /// The Drafts view list: open drafts matching the Drafts filter.
    pub fn draft_list(&self) -> Vec<&Draft> {
        let f = &self.filters[View::Drafts.index()].text;
        self.data
            .open_drafts()
            .filter(|d| matches_filter(f, &[&d.slug, d.title.as_deref().unwrap_or("")]))
            .collect()
    }

    /// The open draft `P` promotes: the selected one, else the first.
    pub fn selected_draft(&self) -> Option<&Draft> {
        let list = self.draft_list();
        self.selected_draft
            .and_then(|id| list.iter().find(|d| d.id == id))
            .or(list.first())
            .copied()
    }

    /// Select order `id` at start-up (spec 1: inside a project directory the
    /// TUI preselects that order). A closed order turns the `a` toggle on so
    /// the selection is visible rather than falling back to the first row.
    pub fn preselect(&mut self, id: i64) {
        if let Some(row) = self.data.order(id) {
            if row.group == crate::data::Group::Closed {
                self.show_closed = true;
            }
            self.selected = Some(id);
            self.detail_scroll.offset = 0;
        }
    }

    /// New snapshot. When the selected order left the list (cancelled,
    /// archived), the selection moves to its nearest neighbour that is
    /// still there instead of jumping to the first row.
    pub fn replace_data(&mut self, data: Snapshot) {
        let before: Vec<i64> = self.order_list().iter().map(|r| r.order.id).collect();
        let at = self
            .selected
            .and_then(|id| before.iter().position(|&x| x == id));
        self.data = data;
        let Some(at) = at else { return };
        let now: Vec<i64> = self.order_list().iter().map(|r| r.order.id).collect();
        if now.contains(&before[at]) {
            return;
        }
        let next = before[at + 1..]
            .iter()
            .chain(before[..at].iter().rev())
            .find(|id| now.contains(id));
        if let Some(&id) = next {
            self.selected = Some(id);
        }
    }

    /// True when the order detail is on screen: full screen, or the right
    /// pane of Orders at `width` columns.
    pub fn detail_shown(&self, width: u16) -> bool {
        self.detail_open || (self.view == View::Orders && width >= WIDE_COLUMNS)
    }

    /// Up/Down in the current list, clamped at both ends.
    fn move_selection(&mut self, delta: isize) {
        self.detail_scroll.offset = 0;
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
                let ids: Vec<i64> = self.draft_list().iter().map(|d| d.id).collect();
                let cur = self.selected_draft().map(|d| d.id);
                self.selected_draft = step(&ids, cur, delta);
            }
            View::Money => {}
        }
    }

    /// A bracketed paste: typed into the focused text field of a form or
    /// into the filter being edited, with newlines as spaces so it can
    /// never submit anything. Dropped everywhere else, so pasted text cannot
    /// run action keys.
    pub fn handle_paste(&mut self, text: &str) {
        let clean: String = text
            .trim_end_matches(['\r', '\n'])
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        match &mut self.popup {
            Some(Popup::Form(form)) => form.paste(&clean),
            Some(_) => {}
            None if self.help_open => {}
            None => {
                if self.filter().editing {
                    self.filter_mut().text.push_str(&clean);
                }
            }
        }
    }

    /// Route one key press. Precedence: action popup, help popup, filter
    /// input, global keys, list and action keys.
    /// `width` is the terminal width, for the adaptive `Enter`.
    pub fn handle_key(&mut self, key: KeyEvent, width: u16) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return Outcome::Quit;
        }
        if ctrl && key.code == KeyCode::Char('l') {
            return Outcome::Redraw;
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
            let chord = key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
            match key.code {
                KeyCode::Esc => *f = Filter::default(),
                KeyCode::Enter => f.editing = false,
                KeyCode::Backspace => {
                    f.text.pop();
                }
                // Readline habits: Ctrl+U clears, Ctrl+W drops a word.
                KeyCode::Char('u') if ctrl => f.text.clear(),
                KeyCode::Char('w') if ctrl => {
                    let kept = f.text.trim_end().rfind(' ').map_or(0, |i| i + 1);
                    f.text.truncate(kept);
                }
                // Other chords (Ctrl+H on some terminals, Alt+x) type nothing.
                KeyCode::Char(_) if chord => {}
                KeyCode::Char(c) => f.text.push(c),
                _ => {}
            }
            return Outcome::None;
        }
        // Ctrl and Alt chords are not keys of this app (Ctrl+C is above).
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Outcome::Unhandled(key);
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
            // Money has no list to filter.
            KeyCode::Char('/') if self.view != View::Money => {
                self.filter_mut().editing = true;
                Outcome::None
            }
            KeyCode::Esc => {
                if self.detail_open {
                    self.detail_open = false;
                    self.detail_scroll.offset = 0;
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
                self.detail_scroll.offset = 0;
                Outcome::None
            }
            KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End
                if self.detail_shown(width) =>
            {
                self.detail_scroll.key(key.code);
                Outcome::None
            }
            KeyCode::Enter if self.view == View::Drafts => {
                actions::draft_notes(self);
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
                self.detail_scroll.offset = 0;
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
    /// JOB.md parses kept between refreshes.
    jobs: JobCache,
}

/// How long to wait for input when the refresh timer is off.
const IDLE_POLL: Duration = Duration::from_secs(60);

/// Redraw interval of the upload progress popup.
const UPLOAD_TICK: Duration = Duration::from_millis(80);

impl App {
    pub fn new(ctx: Ctx, settings: &Tui, theme: Theme) -> Self {
        Self {
            ctx,
            ui: UiState::default(),
            theme,
            icons: Icons::new(settings.icons),
            refresh_every: (settings.refresh_seconds > 0)
                .then(|| Duration::from_secs(settings.refresh_seconds)),
            // (An absurd interval simply never fires; see `run_loop`.)
            clipboard: None,
            jobs: JobCache::default(),
        }
    }

    /// Reload the snapshot. Errors land on the hint line; nothing is retried.
    pub fn refresh(&mut self) {
        match Snapshot::load_cached(&self.ctx, &gig_core::clock::today(), &mut self.jobs) {
            Ok(data) => {
                self.ui.replace_data(data);
                self.ui.error = None;
            }
            Err(e) => self.ui.error = Some(format!("{}: {e}", e.code())),
        }
    }

    pub fn run_loop(&mut self, term: &mut Term) -> Result<()> {
        // `checked_add`: an absurd interval (`--refresh` of centuries) means
        // no timer rather than an overflow panic.
        let mut next_tick = self
            .refresh_every
            .and_then(|d| Instant::now().checked_add(d));
        loop {
            self.draw(term)?;

            let timeout = match next_tick {
                Some(t) => t.saturating_duration_since(Instant::now()),
                None => IDLE_POLL,
            };
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        let width = term.size()?.width;
                        match self.ui.handle_key(key, width) {
                            Outcome::Quit => return Ok(()),
                            Outcome::Refresh => self.refresh(),
                            Outcome::Redraw => term.clear()?,
                            Outcome::Act(effect) => self.apply(effect, term)?,
                            Outcome::None | Outcome::Unhandled(_) => {}
                        }
                    }
                    Event::Paste(text) => self.ui.handle_paste(&text),
                    _ => {}
                }
                // Resize and other events just redraw.
            }
            if let (Some(t), Some(every)) = (next_tick, self.refresh_every) {
                if Instant::now() >= t {
                    self.refresh();
                    next_tick = Instant::now().checked_add(every);
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
            Effect::Call(action) => self.call(&action, term)?,
            Effect::EditField(index) => {
                let initial = actions::field_text(&self.ui, index);
                // A failed editor (`:cq`) leaves the field as it was and
                // keeps the half-filled form. An editor that cannot be
                // started is reported; closing the message brings the form
                // back.
                match terminal::suspend_while(term, || crate::editor::edit_text(&initial))? {
                    Ok(text) => actions::set_edited_field(&mut self.ui, index, text),
                    Err(e) if crate::editor::is_exit_failure(&e) => {}
                    Err(e) => {
                        if let Some(Popup::Form(form)) = self.ui.popup.take() {
                            self.ui.last_form = Some(form);
                        }
                        self.editor_failed(e);
                    }
                }
            }
            Effect::EditNote { slug } => {
                match terminal::suspend_while(term, || crate::editor::edit_text(""))? {
                    // An empty note means "changed my mind".
                    Ok(text) if text.trim().is_empty() => {}
                    Ok(text) => self.call(&Action::Note { slug, text }, term)?,
                    Err(e) => self.editor_failed(e),
                }
            }
            Effect::EditFile(path) => {
                if let Err(e) = terminal::suspend_while(term, || crate::editor::edit_file(&path))? {
                    self.editor_failed(e);
                }
                self.refresh();
            }
            Effect::Copy(link) => {
                let lines = self.copy_lines(link);
                self.ui.popup = Some(Popup::message("link", lines));
            }
            Effect::Upload(job) => self.upload(&job, term)?,
        }
        Ok(())
    }

    /// Confirmed `u` or `U`: build the configured uploader (a secrets or
    /// config error is shown verbatim), run the upload on a worker thread
    /// while this thread draws the progress popup, then show the link and
    /// copy it. Keys pressed meanwhile are dropped: one upload at a time,
    /// and there is no cancel.
    fn upload(&mut self, job: &UploadJob, term: &mut Term) -> Result<()> {
        let uploader = match configured_uploader(&self.ctx.config, &self.ctx.paths) {
            Ok(u) => u,
            Err(e) => {
                self.ui.popup = Some(Popup::error(&e));
                return Ok(());
            }
        };
        let Self {
            ctx,
            ui,
            theme,
            icons,
            ..
        } = self;
        let title = job.title();
        let mut frame_no = 0;
        let mut draw_error = None;
        let result = upload::run_on_worker(ctx, job, uploader.as_ref(), |sent, total| {
            ui.popup = Some(Popup::Progress {
                title: title.clone(),
                sent,
                total,
                frame: frame_no,
            });
            frame_no += 1;
            let drawn = term.draw(|frame| {
                let cx = ui::RenderCx {
                    state: ui,
                    theme,
                    icons,
                };
                ui::draw(frame, &cx);
            });
            if let Err(e) = drawn {
                draw_error.get_or_insert(e);
            }
            // Paces the loop and swallows keys typed during the upload.
            match event::poll(UPLOAD_TICK) {
                Ok(true) => {
                    let _ = event::read();
                }
                Ok(false) => {}
                // A broken input stream returns at once; keep the pace.
                Err(_) => std::thread::sleep(UPLOAD_TICK),
            }
        });
        self.ui.popup = None;
        match result {
            Ok(done) => {
                let mut lines = done.lines();
                if let Some(link) = done.link() {
                    lines.push(String::new());
                    lines.extend(self.copy_lines(link.to_string()));
                }
                self.ui.popup = Some(Popup::message(format!("uploaded {}", done.what), lines));
            }
            Err(e) => self.ui.popup = Some(Popup::error(&e)),
        }
        self.refresh();
        // The upload and its database update succeeded or failed on their
        // own; a draw error must not hide the result popup. The next draw
        // fails too if the terminal is really gone.
        if let Some(e) = draw_error {
            self.ui.error = Some(format!("draw: {e}"));
        }
        Ok(())
    }

    /// Run a gig-core call, then refresh. Slow calls first draw a busy
    /// popup, and keys typed while the UI thread was blocked are dropped so
    /// they cannot act on the result popup.
    fn call(&mut self, action: &Action, term: &mut Term) -> Result<()> {
        let busy = action.busy_text();
        if let Some(text) = busy {
            self.ui.popup = Some(Popup::Busy {
                title: "working".into(),
                text: text.into(),
            });
            self.draw(term)?;
        }
        actions::perform(&self.ctx, &mut self.ui, action);
        if busy.is_some() {
            if let Some(Popup::Busy { .. }) = self.ui.popup {
                self.ui.popup = None;
            }
            while event::poll(Duration::ZERO)? {
                event::read()?;
            }
        }
        self.refresh();
        Ok(())
    }

    fn draw(&self, term: &mut Term) -> Result<()> {
        term.draw(|frame| {
            let cx = ui::RenderCx {
                state: &self.ui,
                theme: &self.theme,
                icons: &self.icons,
            };
            ui::draw(frame, &cx);
        })?;
        Ok(())
    }

    fn editor_failed(&mut self, e: std::io::Error) {
        self.ui.popup = Some(Popup::error_text(
            "editor",
            format!("{}: {e}", crate::editor::command().join(" ")),
        ));
    }

    /// `y` and finished uploads: the link goes to the clipboard; without one
    /// it is only shown. Returns the popup lines saying which.
    fn copy_lines(&mut self, link: String) -> Vec<String> {
        let clip = self
            .clipboard
            .get_or_insert_with(|| arboard::Clipboard::new().map_err(|e| e.to_string()));
        match clip {
            Ok(c) => match c.set_text(link.clone()) {
                // On X11 the copied text lives in this process: without a
                // clipboard manager it is gone once gig tui quits.
                Ok(()) if cfg!(target_os = "linux") => vec![
                    "copied to the clipboard (paste it before quitting gig tui):".into(),
                    link,
                ],
                Ok(()) => vec!["copied to the clipboard:".into(), link],
                Err(e) => vec![format!("clipboard unavailable ({e}):"), link],
            },
            Err(e) => vec![format!("clipboard unavailable ({e}):"), link],
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

    fn with_orders() -> UiState {
        use crate::data::tests::sample_snapshot;
        UiState {
            data: sample_snapshot("2026-09-29"),
            ..UiState::default()
        }
    }

    #[test]
    fn filter_narrows_the_list_actions_use() {
        let mut s = with_orders();
        let all = s.order_list().len();
        assert!(all > 2);
        s.filters[0].text = "O7".into();
        let ids: Vec<i64> = s.order_list().iter().map(|r| r.order.id).collect();
        assert_eq!(ids, vec![7]);
        // A selection hidden by the filter falls back to the first visible row.
        s.selected = Some(6);
        assert_eq!(s.selected_order().unwrap().order.id, 7);
        press(&mut s, KeyCode::Down);
        assert_eq!(s.selected, Some(7));
        // The filter is per view: History is unfiltered.
        s.view = View::History;
        assert_eq!(s.order_list().len(), s.data.orders.len());
    }

    #[test]
    fn chords_are_not_action_keys() {
        let mut s = with_orders();
        for c in ['s', 'n', 'u', 'a'] {
            let chord = KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
            assert!(matches!(s.handle_key(chord, 200), Outcome::Unhandled(_)));
            let alt = KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT);
            assert!(matches!(s.handle_key(alt, 200), Outcome::Unhandled(_)));
        }
        assert!(!s.show_closed);
        assert_eq!(s.popup, None);
        // Shift stays allowed: `N` opens the new order form.
        let shift_n = KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT);
        assert_eq!(s.handle_key(shift_n, 200), Outcome::Act(Effect::None));
        assert!(s.popup.is_some());
    }

    #[test]
    fn preselect_shows_closed_orders() {
        let mut s = with_orders();
        s.preselect(5);
        assert_eq!(s.selected_order().unwrap().order.id, 5);
        assert!(!s.show_closed);
        s.preselect(8); // archived
        assert!(s.show_closed);
        assert_eq!(s.selected_order().unwrap().order.id, 8);
        s.preselect(999);
        assert_eq!(s.selected, Some(8));
    }

    #[test]
    fn selection_moves_to_a_neighbour_when_its_order_leaves() {
        let mut s = with_orders();
        let ids: Vec<i64> = s.order_list().iter().map(|r| r.order.id).collect();
        assert!(ids.len() >= 3);
        s.selected = Some(ids[1]);
        let mut data = s.data.clone();
        data.orders.retain(|r| r.order.id != ids[1]);
        s.replace_data(data);
        assert_eq!(s.selected, Some(ids[2]), "the next row, not the first");
        // The last row falls back to the one above it.
        let last = *s
            .order_list()
            .iter()
            .map(|r| r.order.id)
            .collect::<Vec<_>>()
            .last()
            .unwrap();
        s.selected = Some(last);
        let before: Vec<i64> = s.order_list().iter().map(|r| r.order.id).collect();
        let mut data = s.data.clone();
        data.orders.retain(|r| r.order.id != last);
        s.replace_data(data);
        assert_eq!(s.selected, Some(before[before.len() - 2]));
        let ctrl_l = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL);
        assert_eq!(s.handle_key(ctrl_l, 80), Outcome::Redraw);
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
