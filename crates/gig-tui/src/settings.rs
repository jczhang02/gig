//! The `,` Settings overlay (TUI-SPEC section 8.1): one row per entry of
//! gig-core's settings schema, grouped by section, edited in place. The
//! overlay only holds what it shows; `App::write_setting` writes through
//! gig-core and updates the running dashboard.

use crate::text;
use crate::theme::Theme;
use crate::ui::RenderCx;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gig_core::config::schema::{self, Entry, Kind, SelectSource};
use gig_core::config::Config;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::cell::Cell;
use std::path::Path;

/// Longest typed value; gig-core refuses anything it does not accept.
const EDIT_MAX: usize = 64;

/// Step of `+`/`-` on a number row (the cut ratio).
const NUMBER_STEP: f64 = 0.05;

/// State of the open overlay. Everything is indexed like `schema::ENTRIES`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Row under the cursor.
    pub cursor: usize,
    /// The value being typed into the cursor row (numbers and text).
    pub edit: Option<String>,
    /// The value in config.toml, as a raw string (`true`, `2`, `0.6`, `CNY`).
    pub values: Vec<String>,
    /// The value the running dashboard uses when it differs from the file
    /// (a flag or a `GIG_*` variable overrides it).
    pub session: Vec<Option<String>>,
    /// The last refusal per row, shown under it until the row is edited again.
    pub errors: Vec<Option<String>>,
    /// Where accepted changes go, as shown in the title row.
    pub file: String,
    /// config.toml could not be read or parsed: shown under the title.
    pub load_error: Option<String>,
    /// First drawn line; kept between frames so the list does not jump.
    pub offset: Cell<usize>,
}

/// What a key in the overlay asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    None,
    Close,
    Quit,
    Help,
    /// Open the theme picker (the `tui.theme` row, or `T`).
    Pick,
    /// `r`: reload the data and check the theme files, as in the views.
    Refresh,
    /// Validate and write `raw` at `key` through gig-core.
    Write {
        key: &'static str,
        raw: String,
    },
}

/// The file's config (no `GIG_*` overrides): what the rows show and edit.
/// An absent file is the defaults.
pub fn file_config(path: &Path) -> gig_core::Result<Config> {
    match std::fs::read_to_string(path) {
        Ok(text) => Config::parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(gig_core::Error::PathUnavailable(path.to_path_buf(), e)),
    }
}

/// `key`'s value in `cfg` as the raw string a row shows and edits. An unset
/// theme shows the theme it resolves to.
pub fn raw_value(cfg: &Config, key: &str) -> String {
    if key == "tui.theme" {
        return cfg.tui.theme.clone().unwrap_or_else(|| {
            let name = if cfg.tui.light {
                Theme::LIGHT_NAME
            } else {
                Theme::DEFAULT_NAME
            };
            name.to_string()
        });
    }
    match cfg.get(key) {
        Ok(toml::Value::String(s)) => s,
        Ok(toml::Value::Float(f)) => format_number(f),
        Ok(v) => v.to_string(),
        Err(_) => String::new(),
    }
}

/// `0.6`, `1`, `0.65`: no trailing zeros.
fn format_number(f: f64) -> String {
    let s = format!("{:.4}", f);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// A gig-core error as a row shows it: one line, and for a file that does
/// not parse, the file's name first.
pub fn describe(e: &gig_core::Error, path: &Path) -> String {
    let msg = one_line(&e.to_string());
    match (e, path.file_name()) {
        (gig_core::Error::TomlDe(_), Some(name)) => format!("{}: {msg}", name.to_string_lossy()),
        _ => msg,
    }
}

/// A gig-core error on one line. A TOML parse error spans several lines
/// (the position, the source line, a `^` marker, the reason); a row has
/// room for the position and the reason only:
/// `TOML parse error at line 1, column 5: unclosed table, expected ']'`.
pub fn one_line(msg: &str) -> String {
    let lines: Vec<&str> = msg
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    let art = |l: &str| {
        let t = l.trim_start();
        t.starts_with('|')
            || t.split_once('|')
                .is_some_and(|(n, _)| !n.is_empty() && n.trim().chars().all(|c| c.is_ascii_digit()))
    };
    match lines.as_slice() {
        [] => String::new(),
        [one] => one.to_string(),
        [first, rest @ ..] => {
            let reason: Vec<&str> = rest.iter().filter(|l| !art(l)).map(|l| l.trim()).collect();
            if reason.is_empty() {
                first.to_string()
            } else {
                format!("{first}: {}", reason.join(" "))
            }
        }
    }
}

/// `~/...` for paths under `$HOME`.
pub fn display_path(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

impl Settings {
    /// The overlay over `file` (the file's config, or why it failed) and
    /// `running` (the dashboard's effective config).
    pub fn new(file: gig_core::Result<Config>, running: &Config, path: &Path) -> Self {
        let shown = display_path(path);
        let n = schema::ENTRIES.len();
        let mut s = Self {
            cursor: 0,
            edit: None,
            values: vec![String::new(); n],
            session: vec![None; n],
            errors: vec![None; n],
            file: shown,
            load_error: None,
            offset: Cell::new(0),
        };
        match file {
            Ok(cfg) => s.update(&cfg, running),
            Err(e) => {
                s.load_error = Some(describe(&e, path));
                s.update(running, running);
            }
        }
        s
    }

    /// New values after a write (or on opening).
    pub fn update(&mut self, file: &Config, running: &Config) {
        for (i, e) in schema::ENTRIES.iter().enumerate() {
            let v = raw_value(file, e.key);
            let now = raw_value(running, e.key);
            self.session[i] = (now != v).then_some(now);
            self.values[i] = v;
        }
    }

    /// The running dashboard changed without a write (`M`): only the
    /// `this session` notes move.
    pub fn update_session(&mut self, running: &Config) {
        for (i, e) in schema::ENTRIES.iter().enumerate() {
            let now = raw_value(running, e.key);
            self.session[i] = (now != self.values[i]).then_some(now);
        }
    }

    /// Put the cursor on row `i`. A refusal left under the row the cursor
    /// leaves goes with it: it described a value that was never written.
    pub fn move_to(&mut self, i: usize) {
        let i = i.min(schema::ENTRIES.len() - 1);
        if i != self.cursor {
            if let Some(e) = self.errors.get_mut(self.cursor) {
                *e = None;
            }
            self.cursor = i;
        }
    }

    pub fn entry(&self) -> &'static Entry {
        &schema::ENTRIES[self.cursor.min(schema::ENTRIES.len() - 1)]
    }

    /// Row index of `key`.
    pub fn index(key: &str) -> Option<usize> {
        schema::ENTRIES.iter().position(|e| e.key == key)
    }

    /// A write of `key` was accepted: its error and any typing go.
    pub fn accepted(&mut self, key: &str) {
        if let Some(i) = Self::index(key) {
            self.errors[i] = None;
            if i == self.cursor {
                self.edit = None;
            }
        }
    }

    /// gig-core refused `key`: the message goes under its row, the typed
    /// value stays so it can be fixed.
    pub fn refused(&mut self, key: &str, message: String) {
        if let Some(i) = Self::index(key) {
            self.errors[i] = Some(message);
        }
    }

    pub fn paste(&mut self, text: &str) {
        if let Some(buf) = &mut self.edit {
            for c in text.chars() {
                if buf.chars().count() < EDIT_MAX {
                    buf.push(c);
                }
            }
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> Cmd {
        let chord = key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let e = self.entry();
        let i = self.cursor.min(schema::ENTRIES.len() - 1);
        if let Some(buf) = &mut self.edit {
            match key.code {
                KeyCode::Esc => {
                    self.edit = None;
                    self.errors[i] = None;
                }
                KeyCode::Enter => {
                    return Cmd::Write {
                        key: e.key,
                        raw: buf.clone(),
                    }
                }
                KeyCode::Backspace => {
                    buf.pop();
                    self.errors[i] = None;
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    buf.clear();
                    self.errors[i] = None;
                }
                KeyCode::Char(_) if chord => {}
                KeyCode::Char(c) => {
                    if buf.chars().count() < EDIT_MAX {
                        buf.push(c);
                    }
                    self.errors[i] = None;
                }
                _ => {}
            }
            return Cmd::None;
        }
        if chord {
            return Cmd::None;
        }
        let current = self.values[i].clone();
        let last = schema::ENTRIES.len() - 1;
        match key.code {
            KeyCode::Esc | KeyCode::Char(',') => return Cmd::Close,
            KeyCode::Char('q') => return Cmd::Quit,
            KeyCode::Char('?') => return Cmd::Help,
            KeyCode::Char('T') => return Cmd::Pick,
            KeyCode::Char('r') => return Cmd::Refresh,
            KeyCode::Up | KeyCode::Char('k') => self.move_to(i.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => self.move_to((i + 1).min(last)),
            KeyCode::Home => self.move_to(0),
            KeyCode::End => self.move_to(last),
            KeyCode::Enter | KeyCode::Char(' ') => match e.kind {
                Kind::Toggle => {
                    let on = current == "true";
                    return Cmd::Write {
                        key: e.key,
                        raw: (!on).to_string(),
                    };
                }
                Kind::Select(SelectSource::Theme) => return Cmd::Pick,
                Kind::Select(SelectSource::Static(options)) => {
                    return Cmd::Write {
                        key: e.key,
                        raw: next_option(options, &current).to_string(),
                    };
                }
                Kind::Integer { .. } | Kind::Number { .. } | Kind::Text | Kind::Currency
                    if key.code == KeyCode::Enter =>
                {
                    self.edit = Some(current);
                    self.errors[i] = None;
                }
                _ => {}
            },
            KeyCode::Char(c @ ('+' | '=' | '-')) | KeyCode::Char(c @ ('l' | 'h')) => {
                let up = matches!(c, '+' | '=' | 'l');
                if let Some(raw) = step(e.kind, &current, up) {
                    return Cmd::Write { key: e.key, raw };
                }
            }
            KeyCode::Right | KeyCode::Left => {
                if let Some(raw) = step(e.kind, &current, key.code == KeyCode::Right) {
                    return Cmd::Write { key: e.key, raw };
                }
            }
            // Typing a digit starts a new value.
            KeyCode::Char(c)
                if (c.is_ascii_digit()
                    && matches!(e.kind, Kind::Integer { .. } | Kind::Number { .. }))
                    || (c == '.' && matches!(e.kind, Kind::Number { .. })) =>
            {
                self.edit = Some(c.to_string());
                self.errors[i] = None;
            }
            _ => {}
        }
        Cmd::None
    }
}

fn next_option<'a>(options: &[&'a str], current: &str) -> &'a str {
    let at = options.iter().position(|o| *o == current);
    let next = at.map_or(0, |i| (i + 1) % options.len().max(1));
    options.get(next).copied().unwrap_or_default()
}

/// `+`/`-` on a number row: one up or down. Values past the range are sent
/// anyway so gig-core's refusal names the range.
fn step(kind: Kind, current: &str, up: bool) -> Option<String> {
    match kind {
        Kind::Integer { .. } => {
            let n: i64 = current.trim().parse().unwrap_or(0);
            Some((if up { n + 1 } else { n - 1 }).to_string())
        }
        Kind::Number { .. } => {
            let n: f64 = current.trim().parse().unwrap_or(0.0);
            let v = if up { n + NUMBER_STEP } else { n - NUMBER_STEP };
            Some(format_number((v * 100.0).round() / 100.0))
        }
        // Toggles flip with Space or Enter only.
        _ => None,
    }
}

/// Key hints for the footer while the overlay is open.
pub fn footer_keys(s: &Settings) -> Vec<(&'static str, &'static str)> {
    if s.edit.is_some() {
        return vec![("Enter", "save"), ("Esc", "cancel")];
    }
    match s.entry().kind {
        Kind::Toggle => vec![("Space", "toggle")],
        Kind::Integer { .. } | Kind::Number { .. } => vec![("+ -", "step"), ("Enter", "type")],
        Kind::Select(_) => vec![("Enter", "choose")],
        Kind::Text | Kind::Currency => vec![("Enter", "edit")],
    }
}

/// Widest the overlay draws (label, key, value and a note fit in it).
const MEASURE: u16 = 96;

/// Fewest cells a value keeps when a `now <value>` note needs the room.
const VALUE_MIN: usize = 6;

/// Label column width.
const LABEL_W: usize = 14;
/// The label column is dropped below this body width.
const LABEL_MIN_BODY: usize = 70;

/// One drawn line: its spans and the row it belongs to.
struct Drawn {
    line: Line<'static>,
    row: Option<usize>,
    /// The section heading this line is, if it is one.
    heading: Option<&'static str>,
}

/// The overlay in the view body (the banner, message row and footer stay).
pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx, s: &Settings) {
    let t = cx.theme;
    // A table, not a page: past this measure extra width stays margin.
    let area = Rect {
        width: area.width.min(MEASURE),
        ..area
    };
    let width = usize::from(area.width);
    if width < 4 || area.height == 0 {
        return;
    }
    let key_w = schema::ENTRIES
        .iter()
        .map(|e| text::width(e.key))
        .max()
        .unwrap_or(0);
    let label = width >= LABEL_MIN_BODY;
    let key_x = if label { 2 + LABEL_W + 2 } else { 2 };
    let value_x = key_x + key_w + 2;
    let value_w = width.saturating_sub(value_x);

    // Title row: the name, and where changes are written.
    let mut lines: Vec<Drawn> = Vec::new();
    // `writes <path>`, the path cut from the left so the file name stays.
    let room = width.saturating_sub(8 + 2 + 7);
    let path = format!("writes {}", text::truncate_left(&s.file, room));
    let gap = width.saturating_sub(8 + text::width(&path));
    lines.push(Drawn {
        line: Line::from(vec![
            Span::styled("Settings", t.title()),
            Span::raw(" ".repeat(gap)),
            Span::styled(path, t.muted()),
        ]),
        row: None,
        heading: None,
    });
    if let Some(err) = &s.load_error {
        for l in text::wrap(&format!("! {err}"), width) {
            lines.push(Drawn {
                line: Line::from(Span::styled(l, t.error())),
                row: None,
                heading: None,
            });
        }
    }

    let mut section = "";
    let mut cursor_at = None;
    for (i, e) in schema::ENTRIES.iter().enumerate() {
        if e.section != section {
            section = e.section;
            lines.push(Drawn {
                line: Line::raw(""),
                row: None,
                heading: None,
            });
            lines.push(Drawn {
                line: Line::from(Span::styled(section.to_string(), t.title())),
                row: None,
                heading: Some(e.section),
            });
        }
        let selected = i == s.cursor;
        let marker = || {
            if selected {
                Span::styled(crate::views::SELECTED_MARK, t.accent())
            } else {
                Span::raw(" ")
            }
        };
        // Line 1: label, key path, value.
        let mut spans = vec![marker(), Span::raw(" ")];
        if label {
            let style = if selected { t.title() } else { t.text() };
            spans.push(Span::styled(text::fit(e.label, LABEL_W), style));
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(text::fit(e.key, key_w), t.muted()));
        spans.push(Span::raw("  "));
        let editing = if selected { s.edit.as_deref() } else { None };
        let (mut value, cursor_col) = value_spans(t, e, &s.values[i], editing, value_w);
        let used: usize = value.iter().map(Span::width).sum();
        // `  ·  this session <value>`; when that does not fit, `now <value>`,
        // and the file's value is cut before the session value is.
        let note = match (&s.session[i], editing) {
            (Some(now), None) => {
                let now = display_value(e, now);
                let long = format!("this session {now}");
                let short = format!("now {now}");
                if used + 5 + text::width(&long) <= value_w {
                    Some(long)
                } else {
                    let need = 5 + text::width(&short);
                    if used + need > value_w && value_w >= need + VALUE_MIN {
                        value = value_spans(t, e, &s.values[i], None, value_w - need).0;
                    }
                    Some(short)
                }
            }
            _ => None,
        };
        let used: usize = value.iter().map(Span::width).sum();
        spans.extend(value);
        if let Some(note) = note {
            let room = value_w.saturating_sub(used + 5);
            if room >= 4 {
                spans.push(Span::raw("  "));
                spans.push(Span::styled("\u{b7}", t.dim()));
                spans.push(Span::raw("  "));
                spans.push(Span::styled(text::truncate(&note, room), t.muted()));
            }
        }
        if selected {
            cursor_at = Some((lines.len(), cursor_col.map(|c| value_x + c)));
        }
        lines.push(Drawn {
            line: Line::from(spans),
            row: Some(i),
            heading: None,
        });
        // Line 2: help.
        let help_x = key_x;
        lines.push(Drawn {
            line: Line::from(vec![
                marker(),
                Span::raw(" ".repeat(help_x - 1)),
                Span::styled(
                    text::truncate(e.help, width.saturating_sub(help_x)),
                    t.muted(),
                ),
            ]),
            row: Some(i),
            heading: None,
        });
        // Then the refusal, wrapped.
        if let Some(err) = &s.errors[i] {
            // Continuation lines hang under the text after `! `.
            let wrapped = text::wrap(err, width.saturating_sub(help_x + 2).max(1));
            for (k, l) in wrapped.into_iter().enumerate() {
                let lead = if k == 0 { "! " } else { "  " };
                lines.push(Drawn {
                    line: Line::from(vec![
                        marker(),
                        Span::raw(" ".repeat(help_x - 1)),
                        Span::styled(format!("{lead}{l}"), t.error()),
                    ]),
                    row: Some(i),
                    heading: None,
                });
            }
        }
    }

    // Scroll so the cursor row is whole; the title stays on row 0.
    let title = lines.remove(0);
    let body_h = usize::from(area.height).saturating_sub(1);
    let rows_of = |r: usize| {
        let first = lines.iter().position(|d| d.row == Some(r)).unwrap_or(0);
        let last = lines
            .iter()
            .rposition(|d| d.row == Some(r))
            .unwrap_or(first);
        (first, last)
    };
    let (first, last) = rows_of(s.cursor.min(schema::ENTRIES.len() - 1));
    // Keep the section heading above the first row of a section in view.
    let want_top = if first >= 1 && lines[first - 1].heading.is_some() {
        first - 1
    } else {
        first
    };
    let view = Window::fit(&lines, body_h, s.offset.get(), want_top, last);
    s.offset.set(view.start);

    let sel_band = !t.no_color();
    frame.render_widget(Paragraph::new(title.line), Rect { height: 1, ..area });
    let body_y = area.y + 1 + u16::from(view.top);
    let start = view.start;
    let sticky = if view.top {
        lines[..start.min(lines.len())]
            .iter()
            .rev()
            .find_map(|d| d.heading)
            .filter(|_| lines.get(start).is_some_and(|d| d.heading.is_none()))
    } else {
        None
    };
    for (k, d) in lines.into_iter().skip(start).take(view.len).enumerate() {
        let y = body_y + k as u16;
        let rect = Rect::new(area.x, y, area.width, 1);
        // Mouse: every line of a row (label, help, error) is that row.
        if let Some(i) = d.row {
            cx.state.hits.add(rect, crate::mouse::Target::Setting(i));
        }
        let mut line = d.line;
        if sel_band && d.row == Some(s.cursor) {
            line = line.style(t.selected());
            frame.buffer_mut().set_style(rect, t.selected());
        }
        frame.render_widget(Paragraph::new(line), rect);
    }
    // The scroll indicators have rows of their own; the top one also names
    // the section the first row shown belongs to.
    if view.top {
        let rect = Rect::new(area.x, area.y + 1, area.width, 1);
        if let Some(h) = sticky {
            frame.render_widget(Paragraph::new(Line::from(Span::styled(h, t.title()))), rect);
        }
        let msg = format!("\u{2191} {start} above");
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(msg, t.muted())).right_aligned()),
            rect,
        );
    }
    if view.below > 0 {
        let msg = format!("\u{2193} {} more", view.below);
        let rect = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(msg, t.muted())).right_aligned()),
            rect,
        );
    }
    if let Some((line_no, Some(col))) = cursor_at {
        let body_line = line_no - 1;
        if body_line >= start && body_line < start + view.len && col < width {
            let y = body_y + (body_line - start) as u16;
            frame.set_cursor_position((area.x + col as u16, y));
        }
    }
}

/// The part of the body on screen: `len` lines from `start`, under an
/// `↑ N above` row when `top`, over a `↓ N more` row when `below > 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Window {
    start: usize,
    len: usize,
    top: bool,
    below: usize,
}

impl Window {
    /// The window starting at `start` in `body_h` rows. A section heading
    /// is never the last line shown above `↓ N more` (TUI-DESIGN 9.3).
    fn at(lines: &[Drawn], body_h: usize, start: usize) -> Self {
        let total = lines.len();
        let start = start.min(total);
        // Too short for indicator rows: just the lines.
        if body_h < 3 {
            let len = body_h.min(total - start);
            return Self {
                start,
                len,
                top: false,
                below: total - start - len,
            };
        }
        let top = start > 0;
        let avail = body_h - usize::from(top);
        let mut len = if total - start <= avail {
            total - start
        } else {
            avail - 1
        };
        if start + len < total {
            // Leave out a heading (and the blank above it) at the end.
            while len > 1 {
                let d = &lines[start + len - 1];
                let blank_before_heading = d.row.is_none()
                    && d.heading.is_none()
                    && lines.get(start + len).is_some_and(|n| n.heading.is_some());
                if d.heading.is_some() || blank_before_heading {
                    len -= 1;
                } else {
                    break;
                }
            }
        }
        Self {
            start,
            len,
            top,
            below: total - start - len,
        }
    }

    /// The window nearest to `prev` that shows lines `want_top..=last`
    /// whole, without scrolling further than the end needs.
    fn fit(lines: &[Drawn], body_h: usize, prev: usize, want_top: usize, last: usize) -> Self {
        let total = lines.len();
        // Past this start the end of the list would leave rows empty.
        let max_start = if total <= body_h {
            0
        } else {
            total + 1 - body_h.max(1)
        };
        let mut start = prev.min(max_start).min(want_top);
        let mut w = Self::at(lines, body_h, start);
        while last >= w.start + w.len && start < want_top {
            start += 1;
            w = Self::at(lines, body_h, start);
        }
        w
    }
}

/// A value as a row shows it to a person (`yes`, `gig-dark`, `2`).
fn display_value(e: &Entry, raw: &str) -> String {
    match e.kind {
        Kind::Toggle => if raw == "true" { "yes" } else { "no" }.into(),
        _ => raw.to_string(),
    }
}

/// The value cell by kind (section 12.3 anatomy), and the cursor column
/// inside it while typing.
fn value_spans(
    t: &Theme,
    e: &Entry,
    raw: &str,
    editing: Option<&str>,
    w: usize,
) -> (Vec<Span<'static>>, Option<usize>) {
    if let Some(buf) = editing {
        let shown = text::tail(buf, w.saturating_sub(1));
        let col = text::width(&shown);
        return (vec![Span::styled(shown, t.text())], Some(col));
    }
    let spans = match e.kind {
        Kind::Toggle => {
            let (b, word) = if raw == "true" {
                ("[x]", "yes")
            } else {
                ("[ ]", "no")
            };
            vec![
                Span::styled(b, t.muted()),
                Span::styled(format!(" {word}"), t.text()),
            ]
        }
        Kind::Select(_) => vec![
            Span::styled("\u{2039} ", t.muted()),
            Span::styled(text::truncate(raw, w.saturating_sub(4)), t.text()),
            Span::styled(" \u{203a}", t.muted()),
        ],
        _ if raw.is_empty() => vec![Span::styled(
            "not set",
            t.dim().add_modifier(Modifier::ITALIC),
        )],
        _ => vec![Span::styled(text::truncate(raw, w), t.text())],
    };
    (spans, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn open() -> Settings {
        let cfg = Config::default();
        Settings::new(Ok(cfg.clone()), &cfg, Path::new("/x/config.toml"))
    }

    #[test]
    fn values_come_from_the_file_and_show_session_overrides() {
        let file = Config::default();
        let mut running = file.clone();
        running.tui.icons = false;
        running.tui.theme = Some("nord".into());
        let s = Settings::new(Ok(file), &running, Path::new("/x/config.toml"));
        let i = |k| Settings::index(k).unwrap();
        assert_eq!(s.values[i("tui.theme")], "gig-dark");
        assert_eq!(s.values[i("tui.icons")], "true");
        assert_eq!(s.values[i("tui.refresh_seconds")], "2");
        assert_eq!(s.values[i("general.default_cut_ratio")], "0.6");
        assert_eq!(s.session[i("tui.icons")].as_deref(), Some("false"));
        assert_eq!(s.session[i("tui.theme")].as_deref(), Some("nord"));
        assert_eq!(s.session[i("tui.mouse")], None);
    }

    #[test]
    fn keys_by_kind() {
        let mut s = open();
        // Theme row: Enter opens the picker.
        assert_eq!(s.key(key(KeyCode::Enter)), Cmd::Pick);
        // Icons: Space flips.
        s.key(key(KeyCode::Down));
        assert_eq!(
            s.key(key(KeyCode::Char(' '))),
            Cmd::Write {
                key: "tui.icons",
                raw: "false".into()
            }
        );
        // Refresh: + and - step, digits start typing.
        s.key(key(KeyCode::Down));
        assert_eq!(
            s.key(key(KeyCode::Char('+'))),
            Cmd::Write {
                key: "tui.refresh_seconds",
                raw: "3".into()
            }
        );
        s.key(key(KeyCode::Char('6')));
        s.key(key(KeyCode::Char('1')));
        assert_eq!(s.edit.as_deref(), Some("61"));
        // q types while editing.
        assert_eq!(s.key(key(KeyCode::Char('q'))), Cmd::None);
        s.key(key(KeyCode::Backspace));
        assert_eq!(
            s.key(key(KeyCode::Enter)),
            Cmd::Write {
                key: "tui.refresh_seconds",
                raw: "61".into()
            }
        );
        s.refused("tui.refresh_seconds", "nope".into());
        assert_eq!(s.edit.as_deref(), Some("61"), "typed value kept");
        // Esc cancels the typing first, then closes.
        assert_eq!(s.key(key(KeyCode::Esc)), Cmd::None);
        assert_eq!(s.edit, None);
        assert_eq!(s.errors[2], None);
        // Cut ratio steps by 0.05.
        s.key(key(KeyCode::End));
        assert_eq!(
            s.key(key(KeyCode::Char('-'))),
            Cmd::Write {
                key: "general.default_cut_ratio",
                raw: "0.55".into()
            }
        );
        assert_eq!(s.key(key(KeyCode::Char('q'))), Cmd::Quit);
        assert_eq!(s.key(key(KeyCode::Esc)), Cmd::Close);
    }

    #[test]
    fn numbers_format_without_trailing_zeros() {
        assert_eq!(format_number(0.6), "0.6");
        assert_eq!(format_number(1.0), "1");
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(0.65), "0.65");
        assert_eq!(next_option(&["a", "b"], "b"), "a");
        assert_eq!(next_option(&["a", "b"], "x"), "a");
    }

    #[test]
    fn r_refreshes_and_a_refusal_goes_when_the_cursor_leaves() {
        let mut s = open();
        assert_eq!(s.key(key(KeyCode::Char('r'))), Cmd::Refresh);
        s.cursor = 2;
        s.refused("tui.refresh_seconds", "must be 0 to 60".into());
        s.key(key(KeyCode::Down));
        assert_eq!(s.errors[2], None);
        // While typing, `r` is text.
        s.key(key(KeyCode::Up));
        s.key(key(KeyCode::Enter));
        assert_eq!(s.key(key(KeyCode::Char('r'))), Cmd::None);
    }

    #[test]
    fn m_moves_only_the_session_note() {
        let mut s = open();
        let i = Settings::index("tui.mouse").unwrap();
        let mut running = Config::default();
        running.tui.mouse = false;
        s.update_session(&running);
        assert_eq!(s.values[i], "true");
        assert_eq!(s.session[i].as_deref(), Some("false"));
        s.update_session(&Config::default());
        assert_eq!(s.session[i], None);
    }

    #[test]
    fn toml_errors_fit_on_one_line() {
        let e = Config::parse("[tui\n").unwrap_err();
        let msg = describe(&e, Path::new("/x/config.toml"));
        assert!(!msg.contains('\n'), "{msg}");
        assert!(msg.starts_with("config.toml: "), "{msg}");
        assert!(msg.contains("line 1"), "{msg}");
        assert!(!msg.contains('^') && !msg.contains(" | "), "{msg}");
        assert_eq!(one_line("plain"), "plain");
    }
}
