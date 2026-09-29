//! Application state, global key routing (spec section 2), and the event loop.

use crate::actions::{self, Action, Effect};
use crate::data::{JobCache, OrderRow, Snapshot};
use crate::icons::Icons;
use crate::mouse::{self, Click, Hits, Pane, Target};
use crate::picker::{PickCmd, Picker};
use crate::popup::{Form, Popup, Scroll};
use crate::settings::{self, Cmd, Settings};
use crate::terminal::{self, Term};
use crate::theme::{ColorMode, Theme};
use crate::themes::{Catalog, Stamps};
use crate::ui::{self, WidthClass};
use crate::upload::{self, UploadJob};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use gig_core::config::{Config, Tui};
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

/// Tone of a toast on the message row (TUI-DESIGN.md section 7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// `text`; clears after `TOAST_TTL` or on the next key.
    Info,
    /// `warranty`; clears on the next key.
    Warn,
    /// `unpaid`; clears on the next key.
    Error,
}

/// One message on the right of the message row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub text: String,
    pub tone: Tone,
    /// When an info toast clears by itself.
    pub until: Option<Instant>,
}

/// How long an info toast stays.
pub const TOAST_TTL: Duration = Duration::from_secs(3);

impl Toast {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Info,
            until: Instant::now().checked_add(TOAST_TTL),
        }
    }

    pub fn warn(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Warn,
            until: None,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Error,
            until: None,
        }
    }
}

/// The NOTES.md tail shown by `Enter` in Drafts at Medium and Wide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesPane {
    pub draft_id: i64,
    pub lines: Vec<String>,
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
    /// Scroll of the help popup when it is taller than the terminal.
    pub help_scroll: Scroll,
    /// Full-screen detail (narrow terminals, and History `Enter`).
    pub detail_open: bool,
    /// Last refresh error, shown on the message row until the next success.
    pub error: Option<String>,
    /// Message row toast: a copied link, a theme change, a warning.
    pub toast: Option<Toast>,
    /// Drafts `Enter` at Medium and Wide: the NOTES.md tail pane.
    pub notes_pane: Option<NotesPane>,
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
    /// `,`: the full-screen Settings overlay (TUI-SPEC 8.1).
    pub settings: Option<Settings>,
    /// `T` or the theme row: the theme picker, previewing as it moves.
    pub picker: Option<Picker>,
    /// Money: the chart month (`YYYY-MM`) whose paid orders are listed
    /// under the chart (a bar click). A label, not a slot, so it stays put
    /// when the 12-month window moves on.
    pub money_month: Option<String>,
    /// What the last frame drew where, for mouse events (TUI-SPEC 8.2).
    pub hits: Hits,
    /// The last click, to spot a double-click.
    pub last_click: Option<Click>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            view: View::Orders,
            filters: Default::default(),
            help_open: false,
            help_scroll: Scroll::default(),
            detail_open: false,
            error: None,
            toast: None,
            notes_pane: None,
            data: Snapshot::default(),
            popup: None,
            selected: None,
            selected_draft: None,
            show_closed: false,
            detail_scroll: Scroll::default(),
            last_form: None,
            settings: None,
            picker: None,
            money_month: None,
            hits: Hits::default(),
            last_click: None,
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
    /// `,`: open Settings (the app reads config.toml for it).
    OpenSettings,
    /// `T`, or Enter on the theme row: open the theme picker.
    OpenPicker,
    /// A settings change to validate and write through gig-core.
    WriteSetting {
        key: &'static str,
        raw: String,
    },
    /// `c` in the picker: copy a built-in theme to a file and edit it.
    CopyTheme(String),
    /// `M`: mouse capture on or off for this session (native selection).
    ToggleMouse,
    /// Not a key of this app state: the current view may use it.
    Unhandled(KeyEvent),
}

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
            self.notes_pane = None;
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
        self.detail_open
            || (self.view == View::Orders && WidthClass::of(width) != WidthClass::Narrow)
    }

    /// Delivered, unpaid orders of the Money view, longest waiting first.
    pub fn owed_ids(&self) -> Vec<i64> {
        self.data.money.owed.iter().map(|o| o.order_id).collect()
    }

    /// The selected row of the Money outstanding table.
    pub fn selected_owed(&self) -> Option<i64> {
        let ids = self.owed_ids();
        self.selected
            .filter(|id| ids.contains(id))
            .or(ids.first().copied())
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
                // The notes pane follows the selection.
                if self.notes_pane.is_some() {
                    actions::draft_notes(self, true);
                }
            }
            View::Money => {
                let ids = self.owed_ids();
                let cur = self.selected_owed();
                self.selected = step(&ids, cur, delta);
            }
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
            None if self.picker.is_some() || self.help_open => {}
            None if self.settings.is_some() => {
                if let Some(s) = &mut self.settings {
                    s.paste(&clean);
                }
            }
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
        // Toasts last until the next key.
        self.toast = None;
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
        if let Some(p) = &mut self.picker {
            return match p.key(key) {
                PickCmd::None => Outcome::None,
                PickCmd::Close => {
                    self.picker = None;
                    Outcome::None
                }
                PickCmd::Keep(name) => {
                    self.picker = None;
                    Outcome::WriteSetting {
                        key: "tui.theme",
                        raw: name,
                    }
                }
                PickCmd::Copy(name) => Outcome::CopyTheme(name),
                PickCmd::NotBuiltin(name) => {
                    self.toast = Some(Toast::warn(format!(
                        "c copies a built-in; {name} is a theme file already"
                    )));
                    Outcome::None
                }
            };
        }
        if self.help_open {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => self.help_open = false,
                KeyCode::Char('q') => return Outcome::Quit,
                code => {
                    self.help_scroll.key(code);
                }
            }
            return Outcome::None;
        }
        // `M` everywhere but while typing (TUI-SPEC 8.2).
        let typing =
            self.filter().editing || self.settings.as_ref().is_some_and(|s| s.edit.is_some());
        if key.code == KeyCode::Char('M') && !typing && !ctrl {
            return Outcome::ToggleMouse;
        }
        if let Some(s) = &mut self.settings {
            return match s.key(key) {
                Cmd::None => Outcome::None,
                Cmd::Close => {
                    self.settings = None;
                    Outcome::None
                }
                Cmd::Quit => Outcome::Quit,
                Cmd::Help => {
                    self.help_open = true;
                    self.help_scroll.offset = 0;
                    Outcome::None
                }
                Cmd::Pick => Outcome::OpenPicker,
                Cmd::Write { key, raw } => Outcome::WriteSetting { key, raw },
            };
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
                self.help_scroll.offset = 0;
                Outcome::None
            }
            KeyCode::Char('r') => Outcome::Refresh,
            KeyCode::Char('T') => Outcome::OpenPicker,
            KeyCode::Char(',') => Outcome::OpenSettings,
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
                } else if self.notes_pane.is_some() {
                    self.notes_pane = None;
                } else if self.view == View::Money && self.money_month.is_some() {
                    self.money_month = None;
                } else {
                    *self.filter_mut() = Filter::default();
                }
                Outcome::None
            }
            KeyCode::Enter
                if (self.view == View::Orders && WidthClass::of(width) == WidthClass::Narrow)
                    || self.view == View::History =>
            {
                self.detail_open = true;
                self.detail_scroll.offset = 0;
                Outcome::None
            }
            // Money: the selected outstanding order, full screen.
            KeyCode::Enter if self.view == View::Money => {
                if let Some(id) = self.selected_owed() {
                    self.selected = Some(id);
                    self.detail_open = true;
                    self.detail_scroll.offset = 0;
                }
                Outcome::None
            }
            KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End
                if self.detail_shown(width) =>
            {
                self.detail_scroll.key(key.code);
                Outcome::None
            }
            KeyCode::Enter if self.view == View::Drafts => {
                if self.notes_pane.is_some() {
                    self.notes_pane = None;
                } else {
                    actions::draft_notes(self, WidthClass::of(width) != WidthClass::Narrow);
                }
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

    /// A plain key press, as a hint button or a double-click sends it.
    fn press(&mut self, code: KeyCode, width: u16) -> Outcome {
        self.handle_key(KeyEvent::new(code, KeyModifiers::NONE), width)
    }

    /// Route one mouse event (TUI-SPEC 8.2) against what the last frame
    /// recorded in `hits`. `now` is the event time, passed in so that
    /// double-clicks can be tested. Moves, drags, releases and the other
    /// buttons do nothing.
    pub fn handle_mouse(&mut self, ev: MouseEvent, now: Instant, width: u16) -> Outcome {
        let map = self.hits.map();
        let (x, y) = (ev.column, ev.row);
        match ev.kind {
            MouseEventKind::ScrollUp => self.wheel(map.pane_at(x, y), -1),
            MouseEventKind::ScrollDown => self.wheel(map.pane_at(x, y), 1),
            MouseEventKind::Down(MouseButton::Left) => {
                // A click is a key for the toast: it clears it.
                self.toast = None;
                if map.outside_modal(x, y) {
                    // Outside a popup: exactly what Esc does there.
                    self.last_click = None;
                    return self.press(KeyCode::Esc, width);
                }
                let Some(target) = map.target_at(x, y).cloned() else {
                    self.last_click = None;
                    return Outcome::None;
                };
                let double = mouse::is_double(self.last_click.as_ref(), &target, now);
                // The second click of a double-click does not start another.
                self.last_click = (!double).then(|| Click {
                    at: now,
                    target: target.clone(),
                });
                self.click(target, double, width)
            }
            _ => Outcome::None,
        }
    }

    fn click(&mut self, target: Target, double: bool, width: u16) -> Outcome {
        match target {
            Target::Tab(v) => {
                // A tab leaves Settings and the full-screen detail as well.
                self.settings = None;
                self.switch(v);
            }
            Target::Key(key) => return self.handle_key(key, width),
            Target::Order(id) => {
                if self.selected != Some(id) {
                    self.selected = Some(id);
                    self.detail_scroll.offset = 0;
                }
                if double {
                    self.detail_open = true;
                    self.detail_scroll.offset = 0;
                }
            }
            Target::Draft(id) => {
                self.selected_draft = Some(id);
                if double && self.notes_pane.is_none() {
                    actions::draft_notes(self, WidthClass::of(width) != WidthClass::Narrow);
                } else if self.notes_pane.is_some() {
                    // The notes pane follows the selection.
                    actions::draft_notes(self, true);
                }
            }
            Target::Owed(id) | Target::Paid(id) => self.jump(id),
            Target::Month(i) => {
                let label = self.data.money.by_month.get(i).map(|m| m.label.clone());
                self.money_month = if self.money_month == label {
                    None
                } else {
                    label
                };
            }
            Target::Link(url) => return Outcome::Act(Effect::Copy(url)),
            Target::Setting(i) => {
                if let Some(s) = &mut self.settings {
                    if s.cursor != i {
                        if s.edit.is_some() {
                            // Leaving a row being typed cancels, as Esc.
                            s.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
                        }
                        s.cursor = i;
                    }
                }
                if double {
                    return self.press(KeyCode::Enter, width);
                }
            }
            Target::Theme(i) => {
                if let Some(p) = &mut self.picker {
                    if p.rows.get(i).is_some_and(|r| r.theme.is_some()) {
                        p.cursor = i;
                    }
                }
                if double {
                    return self.press(KeyCode::Enter, width);
                }
            }
            Target::Field(i) => {
                let focused = match &self.popup {
                    Some(Popup::Form(f)) => f.focus == i,
                    _ => return Outcome::None,
                };
                if !focused {
                    actions::focus_field(self, i);
                    return Outcome::None;
                }
                // A click on the focused field works it: a toggle flips, a
                // select moves on, an $EDITOR field opens the editor.
                let code = match &self.popup {
                    Some(Popup::Form(f)) => match f.fields.get(i).map(|f| &f.kind) {
                        Some(crate::popup::FieldKind::Toggle(_))
                        | Some(crate::popup::FieldKind::Select { .. }) => KeyCode::Char(' '),
                        Some(crate::popup::FieldKind::Editor(_)) => KeyCode::Enter,
                        _ => return Outcome::None,
                    },
                    _ => return Outcome::None,
                };
                return self.press(code, width);
            }
            Target::Item(i) => {
                if let Some(Popup::Pick(p)) = &mut self.popup {
                    if i < p.items.len() {
                        p.selected = i;
                    }
                }
                if double {
                    return self.press(KeyCode::Enter, width);
                }
            }
        }
        Outcome::None
    }

    /// The wheel over `pane`: lists move their selection (a list has no
    /// scroll of its own; its window follows the selection), bodies scroll.
    fn wheel(&mut self, pane: Option<Pane>, delta: isize) -> Outcome {
        let code = if delta < 0 {
            KeyCode::Up
        } else {
            KeyCode::Down
        };
        let rows = delta as i32 * i32::from(mouse::WHEEL_ROWS);
        let key = KeyEvent::new(code, KeyModifiers::NONE);
        match pane {
            Some(Pane::List) => self.move_selection(delta),
            Some(Pane::Detail) => self.detail_scroll.by(rows),
            Some(Pane::Help) => self.help_scroll.by(rows),
            Some(Pane::Settings) => {
                if let Some(s) = self.settings.as_mut().filter(|s| s.edit.is_none()) {
                    s.key(key);
                }
            }
            Some(Pane::Picker) => {
                if let Some(p) = &mut self.picker {
                    p.key(key);
                }
            }
            Some(Pane::Popup) => match &mut self.popup {
                Some(Popup::Confirm { scroll, .. } | Popup::Message { scroll, .. }) => {
                    scroll.by(rows)
                }
                Some(p @ Popup::Pick(_)) => {
                    p.handle_key(key);
                }
                _ => {}
            },
            None => {}
        }
        Outcome::None
    }

    /// Show order `id` where it lives: Orders, or History once it is closed
    /// (an outstanding row or a drill-down row was clicked). A filter that
    /// would hide it is cleared.
    fn jump(&mut self, id: i64) {
        let open = self
            .data
            .active_orders(false)
            .iter()
            .any(|r| r.order.id == id);
        self.switch(if open { View::Orders } else { View::History });
        if !self.order_list().iter().any(|r| r.order.id == id) {
            *self.filter_mut() = Filter::default();
        }
        self.selected = Some(id);
        self.detail_scroll.offset = 0;
    }
}

/// The running dashboard: the one database handle plus the UI state.
pub struct App {
    pub ctx: Ctx,
    pub ui: UiState,
    pub theme: Theme,
    /// Every available theme and the files that failed to load, as read
    /// from the themes directory (reloaded when a file changes).
    pub catalog: Catalog,
    /// How this terminal draws colour; applied to every theme.
    pub mode: ColorMode,
    /// The running `[tui]` settings: config, flags and env at start, then
    /// every change accepted in Settings.
    pub running: Tui,
    pub icons: Icons,
    /// `None` when `refresh_seconds` is 0.
    pub refresh_every: Option<Duration>,
    /// When the refresh timer fires next; `None` while it is off.
    pub next_tick: Option<Instant>,
    /// Theme file modification times at the last check (hot reload).
    stamps: Stamps,
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
    /// `theme` is the one picked from `catalog` for `settings.theme`, already
    /// drawn in `mode`.
    pub fn new(ctx: Ctx, settings: &Tui, theme: Theme, catalog: Catalog, mode: ColorMode) -> Self {
        let stamps = Stamps::scan(&ctx.paths.themes_dir());
        let mut running = settings.clone();
        running.theme = Some(theme.name.to_string());
        let mut app = Self {
            ctx,
            ui: UiState::default(),
            theme,
            catalog,
            mode,
            running,
            icons: Icons::new(settings.icons),
            refresh_every: None,
            next_tick: None,
            stamps,
            clipboard: None,
            jobs: JobCache::default(),
        };
        app.set_refresh(settings.refresh_seconds);
        app
    }

    /// Start (or stop, at 0) the refresh timer. An absurd interval
    /// (`--refresh` of centuries) means no timer rather than an overflow.
    fn set_refresh(&mut self, seconds: u64) {
        self.refresh_every = (seconds > 0).then(|| Duration::from_secs(seconds));
        self.next_tick = self
            .refresh_every
            .and_then(|d| Instant::now().checked_add(d));
    }

    /// The running config as Settings compares it with the file.
    fn running_config(&self) -> Config {
        Config {
            general: self.ctx.config.general.clone(),
            tui: self.running.clone(),
            ..Config::default()
        }
    }

    /// Mouse capture on or off for the running dashboard. A terminal that
    /// refuses is reported on the message row.
    pub fn set_mouse(&mut self, on: bool) {
        self.running.mouse = on;
        self.ui.last_click = None;
        if let Err(e) = terminal::set_mouse(on) {
            self.ui.toast = Some(Toast::error(format!("mouse: {e}")));
        }
    }

    /// `,`: read config.toml and open the overlay.
    pub fn open_settings(&mut self) {
        let path = self.ctx.paths.config_file.clone();
        let file = settings::file_config(&path);
        self.ui.settings = Some(Settings::new(file, &self.running_config(), &path));
    }

    /// `T` or the theme row: the picker over the current catalog.
    pub fn open_picker(&mut self) {
        self.ui.picker = Some(Picker::new(&self.catalog, &self.theme.name, self.mode));
    }

    /// One settings change: gig-core validates and writes it in place; a
    /// refusal goes under the row (or to the message row when Settings is
    /// closed) and nothing changes. An accepted value takes effect at once,
    /// over any flag or `GIG_*` variable that set it at start.
    pub fn write_setting(&mut self, key: &'static str, raw: &str) {
        let file = match Config::set_in_file(&self.ctx.paths.config_file, key, raw) {
            Ok(cfg) => cfg,
            Err(e) => {
                let msg = e.to_string();
                match &mut self.ui.settings {
                    Some(s) if Settings::index(key).is_some() => s.refused(key, msg),
                    _ => self.ui.toast = Some(Toast::error(msg)),
                }
                return;
            }
        };
        let mut warning = None;
        match key {
            "tui.theme" => {
                let (theme, w) = self.catalog.pick(file.tui.theme.as_deref());
                self.theme = theme.for_mode(self.mode);
                self.running.theme = Some(self.theme.name.to_string());
                warning = w;
            }
            "tui.icons" => {
                self.running.icons = file.tui.icons;
                self.icons = Icons::new(file.tui.icons);
            }
            "tui.refresh_seconds" => {
                self.running.refresh_seconds = file.tui.refresh_seconds;
                self.set_refresh(file.tui.refresh_seconds);
            }
            "tui.mouse" => self.set_mouse(file.tui.mouse),
            _ => {
                // `general.*`: the services read ctx.config, so new orders
                // pick the value up at once.
                if let Err(e) = self.ctx.config.set(key, raw) {
                    warning = Some(e.to_string());
                }
            }
        }
        let running = self.running_config();
        if let Some(s) = &mut self.ui.settings {
            s.accepted(key);
            s.update(&file, &running);
        }
        let shown = settings::raw_value(&file, key);
        self.ui.toast = Some(match warning {
            Some(w) => Toast::warn(w),
            None => Toast::info(format!("saved {key} = {shown}")),
        });
    }

    /// `c` in the picker: write `<themes_dir>/<name>-copy.toml` (an existing
    /// copy is kept) and return it for `$EDITOR`. A failure is shown and
    /// gives `None`.
    pub fn copy_theme(&mut self, name: &str) -> Option<std::path::PathBuf> {
        match crate::themes::copy_builtin(&self.ctx.paths.themes_dir(), name) {
            Ok((path, _)) => Some(path),
            Err(e) => {
                self.ui.toast = Some(Toast::error(format!("theme copy: {e}")));
                None
            }
        }
    }

    /// After `$EDITOR` closed on a copied theme: reload the catalog and put
    /// the picker's cursor on the copy, previewing it.
    pub fn after_theme_edit(&mut self, path: &std::path::Path) {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.reload_themes();
        if let Some(p) = &self.ui.picker {
            self.ui.picker = Some(p.rebuilt(&self.catalog, &name, self.mode));
        }
        if let Some(b) = self.catalog.broken.iter().find(|b| b.name == name) {
            self.ui.toast = Some(Toast::warn(b.reason()));
        } else {
            self.ui.toast = Some(Toast::info(format!("theme {name}  \u{b7}  Enter keeps it")));
        }
    }

    /// The refresh tick: new data, then hot reload of theme files.
    pub fn tick(&mut self) {
        self.refresh();
        self.check_themes();
    }

    /// Hot reload (TUI-SPEC 8.1): when a file in the themes directory was
    /// added, removed or rewritten since the last check, reload the catalog;
    /// a changed current theme applies at once, a current file that broke
    /// keeps the colours loaded before and says why.
    pub fn check_themes(&mut self) {
        let stamps = Stamps::scan(&self.ctx.paths.themes_dir());
        if stamps == self.stamps {
            return;
        }
        self.stamps = stamps;
        self.reload_themes();
        if let Some(p) = &self.ui.picker {
            let at = p.highlighted().unwrap_or_default().to_string();
            self.ui.picker = Some(p.rebuilt(&self.catalog, &at, self.mode));
        }
        let name = self.theme.name.to_string();
        if !self.catalog.user.contains(&name) {
            if let Some(b) = self.catalog.broken.iter().find(|b| b.name == name) {
                self.ui.toast = Some(Toast::warn(format!(
                    "{}, keeping the colours loaded before",
                    b.reason()
                )));
            }
            return;
        }
        if let Some(t) = self.catalog.get(&name) {
            let t = t.for_mode(self.mode);
            if t != self.theme {
                self.theme = t;
                let note = match self.theme.contrast_failure() {
                    Some(f) => Toast::warn(format!("theme {name} reloaded: {f}")),
                    None => Toast::info(format!("theme {name} reloaded")),
                };
                self.ui.toast = Some(note);
            }
        }
    }

    fn reload_themes(&mut self) {
        let dir = self.ctx.paths.themes_dir();
        self.stamps = Stamps::scan(&dir);
        self.catalog = Catalog::load(&dir);
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
        let mut redraw = true;
        loop {
            if redraw {
                self.draw(term)?;
            }
            redraw = true;

            let toast_until = self.ui.toast.as_ref().and_then(|t| t.until);
            let wake = match (self.next_tick, toast_until) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let timeout = match wake {
                Some(t) => t.saturating_duration_since(Instant::now()),
                None => IDLE_POLL,
            };
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        let width = term.size()?.width;
                        let outcome = self.ui.handle_key(key, width);
                        if self.outcome(outcome, term)? {
                            return Ok(());
                        }
                    }
                    Event::Paste(text) => self.ui.handle_paste(&text),
                    // Pointer motion arrives often and changes nothing:
                    // no redraw for it.
                    Event::Mouse(m)
                        if !matches!(
                            m.kind,
                            MouseEventKind::Down(MouseButton::Left)
                                | MouseEventKind::ScrollUp
                                | MouseEventKind::ScrollDown
                        ) =>
                    {
                        redraw = false;
                    }
                    Event::Mouse(m) => {
                        let width = term.size()?.width;
                        let outcome = self.ui.handle_mouse(m, Instant::now(), width);
                        if self.outcome(outcome, term)? {
                            return Ok(());
                        }
                    }
                    _ => {}
                }
                // Resize and other events just redraw.
            }
            if toast_until.is_some_and(|t| Instant::now() >= t) {
                self.ui.toast = None;
                redraw = true;
            }
            if let (Some(t), Some(every)) = (self.next_tick, self.refresh_every) {
                if Instant::now() >= t {
                    self.tick();
                    self.next_tick = Instant::now().checked_add(every);
                    redraw = true;
                }
            }
        }
    }

    /// Carry out the outcomes that need no terminal (settings, the theme
    /// picker, refresh); the others are handed back.
    pub fn settle(&mut self, outcome: Outcome) -> Option<Outcome> {
        match outcome {
            Outcome::Refresh => {
                self.refresh();
                self.check_themes();
            }
            Outcome::OpenSettings => self.open_settings(),
            Outcome::OpenPicker => self.open_picker(),
            Outcome::WriteSetting { key, raw } => self.write_setting(key, &raw),
            Outcome::ToggleMouse => {
                let on = !self.running.mouse;
                self.set_mouse(on);
                if self.ui.toast.is_none() {
                    self.ui.toast = Some(Toast::info(if on {
                        "mouse on"
                    } else {
                        "mouse off, the terminal selects text; M turns it on"
                    }));
                }
            }
            Outcome::None | Outcome::Unhandled(_) => {}
            other => return Some(other),
        }
        None
    }

    /// Carry out what a key asked for; true means quit.
    fn outcome(&mut self, outcome: Outcome, term: &mut Term) -> Result<bool> {
        let Some(outcome) = self.settle(outcome) else {
            return Ok(false);
        };
        match outcome {
            Outcome::Quit => return Ok(true),
            Outcome::Redraw => term.clear()?,
            Outcome::Act(effect) => self.apply(effect, term)?,
            Outcome::CopyTheme(name) => {
                if let Some(path) = self.copy_theme(&name) {
                    match terminal::suspend_while(term, || crate::editor::edit_file(&path))? {
                        Ok(()) => {}
                        Err(e) if crate::editor::is_exit_failure(&e) => {}
                        Err(e) => self.editor_failed(e),
                    }
                    self.after_theme_edit(&path);
                }
            }
            _ => {}
        }
        Ok(false)
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
                let shown = crate::text::strip_scheme(&link).to_string();
                self.ui.toast = Some(match self.copy(link) {
                    Ok(()) => Toast::info(format!("copied {shown}")),
                    Err(e) => Toast::error(format!("clipboard unavailable ({e}): {shown}")),
                });
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
                    lines.push(crate::text::strip_scheme(link).to_string());
                    lines.push(match self.copy(link.to_string()) {
                        Ok(()) => "copied to clipboard".into(),
                        Err(_) => "clipboard unavailable, link shown above".into(),
                    });
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
        self.draw_on(term)?;
        Ok(())
    }

    /// Draw one frame on any backend (the tests use `TestBackend`); this
    /// also records the mouse regions of the frame.
    pub fn draw_on<B: ratatui::backend::Backend>(
        &self,
        term: &mut ratatui::Terminal<B>,
    ) -> std::result::Result<(), B::Error> {
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

    /// `y` and finished uploads: the link to the clipboard. On X11 the
    /// copied text lives in this process, which is why the handle is kept.
    fn copy(&mut self, link: String) -> std::result::Result<(), String> {
        let clip = self
            .clipboard
            .get_or_insert_with(|| arboard::Clipboard::new().map_err(|e| e.to_string()));
        match clip {
            Ok(c) => c.set_text(link).map_err(|e| e.to_string()),
            Err(e) => Err(e.clone()),
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
        // Scroll keys move the help when it does not fit.
        s.help_scroll.max.set(3);
        press(&mut s, KeyCode::Down);
        press(&mut s, KeyCode::PageDown);
        assert_eq!(s.help_scroll.offset, 3);
        press(&mut s, KeyCode::Esc);
        assert!(!s.help_open);
        press(&mut s, KeyCode::Char('?'));
        assert_eq!(s.help_scroll.offset, 0);
        press(&mut s, KeyCode::Esc);
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
    fn t_opens_the_picker_and_money_enter_opens_the_order() {
        let mut s = with_orders();
        assert_eq!(press(&mut s, KeyCode::Char('T')), Outcome::OpenPicker);
        assert_eq!(press(&mut s, KeyCode::Char(',')), Outcome::OpenSettings);
        press(&mut s, KeyCode::Char('3'));
        // Owed: o7 (59 days), o6 (1 day), o10 (unknown).
        assert_eq!(s.selected_owed(), Some(7));
        press(&mut s, KeyCode::Down);
        assert_eq!(s.selected_owed(), Some(6));
        press(&mut s, KeyCode::Enter);
        assert!(s.detail_open);
        assert_eq!(s.selected_order().unwrap().order.id, 6);
        press(&mut s, KeyCode::Esc);
        assert!(!s.detail_open);
        assert_eq!(s.view, View::Money);
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
