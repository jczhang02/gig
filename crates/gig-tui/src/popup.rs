//! Action popups: forms, confirmations, and messages (results and verbatim
//! gig-core refusals). Pure state plus drawing; the gig-core calls and the
//! $EDITOR round trip live in `actions` and `app`.

use crate::actions::{Effect, FormKind};
use crate::text;
use crate::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use ratatui::Frame;
use std::cell::Cell;

/// One input of a form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub label: String,
    pub kind: FieldKind,
    /// Shown but not editable (the slug when promoting a draft).
    pub locked: bool,
    /// Validation message drawn under the field; submit is refused
    /// while any field has one.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// Single line typed in place.
    Text(String),
    /// One of `options`; Left/Right/Space cycle.
    Select { options: Vec<String>, idx: usize },
    /// Space toggles.
    Toggle(bool),
    /// Multi-line text edited in $EDITOR (Enter opens it).
    Editor(String),
}

impl Field {
    pub fn text(label: &str, value: impl Into<String>) -> Self {
        Self::new(label, FieldKind::Text(value.into()))
    }

    pub fn select(label: &str, options: &[&str], selected: &str) -> Self {
        let idx = options.iter().position(|o| *o == selected).unwrap_or(0);
        Self::new(
            label,
            FieldKind::Select {
                options: options.iter().map(|s| s.to_string()).collect(),
                idx,
            },
        )
    }

    pub fn toggle(label: &str, on: bool) -> Self {
        Self::new(label, FieldKind::Toggle(on))
    }

    pub fn editor(label: &str, value: impl Into<String>) -> Self {
        Self::new(label, FieldKind::Editor(value.into()))
    }

    fn new(label: &str, kind: FieldKind) -> Self {
        Self {
            label: label.to_string(),
            kind,
            locked: false,
            error: None,
        }
    }

    pub fn locked(mut self) -> Self {
        self.locked = true;
        self
    }

    /// Text value of the field: the typed text, the chosen option, or
    /// "yes"/"no" for a toggle.
    pub fn value(&self) -> &str {
        match &self.kind {
            FieldKind::Text(s) | FieldKind::Editor(s) => s,
            FieldKind::Select { options, idx } => options.get(*idx).map_or("", String::as_str),
            FieldKind::Toggle(on) => {
                if *on {
                    "yes"
                } else {
                    "no"
                }
            }
        }
    }

    pub fn is_on(&self) -> bool {
        matches!(self.kind, FieldKind::Toggle(true))
    }
}

/// A form: labelled fields, one focused, submitted with Enter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub title: String,
    pub kind: FormKind,
    pub fields: Vec<Field>,
    pub focus: usize,
}

impl Form {
    pub fn new(title: impl Into<String>, kind: FormKind, fields: Vec<Field>) -> Self {
        let mut f = Self {
            title: title.into(),
            kind,
            fields,
            focus: 0,
        };
        if f.fields.first().is_some_and(|x| x.locked) {
            f.move_focus(1);
        }
        f
    }

    /// Value of the field labelled `label`; empty when absent.
    pub fn get(&self, label: &str) -> &str {
        self.field(label).map_or("", Field::value)
    }

    pub fn field(&self, label: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.label == label)
    }

    pub fn field_mut(&mut self, label: &str) -> Option<&mut Field> {
        self.fields.iter_mut().find(|f| f.label == label)
    }

    /// Replace the focused-or-named field's text (used after $EDITOR returns).
    pub fn set_text(&mut self, idx: usize, value: String) {
        if let Some(f) = self.fields.get_mut(idx) {
            match &mut f.kind {
                FieldKind::Text(s) | FieldKind::Editor(s) => *s = value,
                _ => {}
            }
        }
    }

    /// Pasted text (newlines already removed) into the focused text field.
    pub fn paste(&mut self, text: &str) {
        if let Some(f) = self.fields.get_mut(self.focus) {
            if let (false, FieldKind::Text(s)) = (f.locked, &mut f.kind) {
                s.push_str(text);
            }
        }
    }

    /// Move to the next (or previous) unlocked field, wrapping.
    fn move_focus(&mut self, delta: isize) {
        let n = self.fields.len();
        if n == 0 {
            return;
        }
        for step in 1..=n {
            let i = (self.focus as isize + delta * step as isize).rem_euclid(n as isize) as usize;
            if !self.fields[i].locked {
                self.focus = i;
                return;
            }
        }
    }

    fn key(&mut self, key: KeyEvent) -> PopupKey {
        match key.code {
            KeyCode::Tab | KeyCode::Down => self.move_focus(1),
            KeyCode::BackTab | KeyCode::Up => self.move_focus(-1),
            KeyCode::Enter => {
                return match self.fields.get(self.focus).map(|f| &f.kind) {
                    Some(FieldKind::Editor(_)) => PopupKey::EditField(self.focus),
                    _ => PopupKey::Submit,
                };
            }
            code => {
                let Some(f) = self.fields.get_mut(self.focus) else {
                    return PopupKey::None;
                };
                if f.locked {
                    return PopupKey::None;
                }
                f.error = None;
                match (&mut f.kind, code) {
                    (FieldKind::Text(s), KeyCode::Char(c))
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        s.push(c)
                    }
                    (FieldKind::Text(s), KeyCode::Backspace) => {
                        s.pop();
                    }
                    (FieldKind::Select { options, idx }, KeyCode::Right | KeyCode::Char(' ')) => {
                        *idx = (*idx + 1) % options.len().max(1)
                    }
                    (FieldKind::Select { options, idx }, KeyCode::Left) => {
                        let n = options.len().max(1);
                        *idx = (*idx + n - 1) % n
                    }
                    (
                        FieldKind::Toggle(on),
                        KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right,
                    ) => *on = !*on,
                    (FieldKind::Editor(_), KeyCode::Char('e')) => {
                        return PopupKey::EditField(self.focus)
                    }
                    _ => {}
                }
            }
        }
        PopupKey::None
    }
}

/// What a pick list is for, and the order it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickFor {
    /// `u`: upload the picked package.
    Upload { slug: String },
    /// `m`: mark the picked package sent.
    MarkSent { slug: String },
}

/// Choose one item with Up/Down and Enter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub title: String,
    pub purpose: PickFor,
    /// `(value, label)`: the value goes to the action, the label is shown.
    pub items: Vec<(String, String)>,
    pub selected: usize,
}

impl Pick {
    pub fn value(&self) -> Option<&str> {
        self.items.get(self.selected).map(|(v, _)| v.as_str())
    }

    fn key(&mut self, key: KeyEvent) -> PopupKey {
        let last = self.items.len().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::BackTab => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => self.selected = (self.selected + 1).min(last),
            KeyCode::Enter if !self.items.is_empty() => return PopupKey::Submit,
            _ => {}
        }
        PopupKey::None
    }
}

/// Vertical scroll of a long popup body. `max` is written by the renderer
/// (it knows the wrapped height), so keys clamp against the last drawn size.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scroll {
    pub offset: u16,
    pub max: Cell<u16>,
}

impl Scroll {
    /// Up/Down/PgUp/PgDn/Home/End (and j/k); true when the key was a
    /// scroll key. An offset past the end (set to open at the bottom)
    /// counts as the end.
    pub(crate) fn key(&mut self, code: KeyCode) -> bool {
        let max = self.max.get();
        self.offset = self.offset.min(max);
        let page = 10;
        self.offset = match code {
            KeyCode::Up | KeyCode::Char('k') => self.offset.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.offset.saturating_add(1),
            KeyCode::PageUp => self.offset.saturating_sub(page),
            KeyCode::PageDown => self.offset.saturating_add(page),
            KeyCode::Home => 0,
            KeyCode::End => max,
            _ => return false,
        }
        .min(max);
        true
    }
}

/// What a popup shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Popup {
    Form(Form),
    Pick(Pick),
    /// A dangerous action waits for a typed `y`; scroll keys scroll, any
    /// other key cancels.
    Confirm {
        title: String,
        lines: Vec<String>,
        /// Carried out on `y`.
        then: Effect,
        scroll: Scroll,
        /// Destructive (cancel): the title is drawn in the error colour.
        /// Routine steps (upload, mark sent) are not.
        danger: bool,
    },
    /// A running upload. Drawn by the blocking upload loop, which reads no
    /// keys, so it never closes by key.
    Progress {
        title: String,
        /// Bytes sent and total; total 0 means the uploader has not reported
        /// (a single PUT reports only at the end): show a spinner.
        sent: u64,
        total: u64,
        /// Spinner frame counter.
        frame: usize,
    },
    /// Drawn once before a slow gig-core call on the UI thread (a dry run
    /// that hashes a package, an archive preview, a scaffold); replaced by
    /// the result when the call returns.
    Busy {
        title: String,
        text: String,
    },
    /// Result or information; Enter/Esc/q/Space closes, scroll keys scroll.
    Message {
        title: String,
        lines: Vec<String>,
        error: bool,
        /// A write that succeeded: the title is drawn in the ok colour.
        ok: bool,
        scroll: Scroll,
    },
}

/// What a key press in a popup asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupKey {
    None,
    Close,
    /// Enter in a form.
    Submit,
    /// Open $EDITOR on the form field at this index.
    EditField(usize),
    /// `y` in a confirmation.
    Confirmed,
}

impl Popup {
    pub fn message(title: impl Into<String>, lines: Vec<String>) -> Self {
        Popup::Message {
            title: title.into(),
            lines,
            error: false,
            ok: false,
            scroll: Scroll::default(),
        }
    }

    /// Result of a write that succeeded.
    pub fn done(title: impl Into<String>, lines: Vec<String>) -> Self {
        match Self::message(title, lines) {
            Popup::Message {
                title,
                lines,
                scroll,
                ..
            } => Popup::Message {
                title,
                lines,
                error: false,
                ok: true,
                scroll,
            },
            other => other,
        }
    }

    /// A message opened scrolled to its end (the newest lines of a tail).
    pub fn message_at_end(title: impl Into<String>, lines: Vec<String>) -> Self {
        let mut p = Self::message(title, lines);
        if let Popup::Message { scroll, .. } = &mut p {
            scroll.offset = u16::MAX;
        }
        p
    }

    /// A refusal or failure that is not a gig-core error (editor, missing
    /// file), shown in the error colour.
    pub fn error_text(title: impl Into<String>, text: impl Into<String>) -> Self {
        Popup::Message {
            title: title.into(),
            lines: vec![text.into()],
            error: true,
            ok: false,
            scroll: Scroll::default(),
        }
    }

    /// A gig-core refusal, shown verbatim with its code (as the hint line
    /// shows refresh errors).
    pub fn error(e: &gig_core::Error) -> Self {
        Popup::Message {
            title: "refused".into(),
            lines: vec![e.code().to_string(), e.to_string()],
            error: true,
            ok: false,
            scroll: Scroll::default(),
        }
    }

    /// A routine confirmation (upload, mark sent).
    pub fn confirm(title: impl Into<String>, lines: Vec<String>, then: Effect) -> Self {
        Popup::Confirm {
            title: title.into(),
            lines,
            then,
            scroll: Scroll::default(),
            danger: false,
        }
    }

    /// A destructive confirmation (cancel an order).
    pub fn confirm_danger(title: impl Into<String>, lines: Vec<String>, then: Effect) -> Self {
        match Self::confirm(title, lines, then) {
            Popup::Confirm {
                title,
                lines,
                then,
                scroll,
                ..
            } => Popup::Confirm {
                title,
                lines,
                then,
                scroll,
                danger: true,
            },
            other => other,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> PopupKey {
        if matches!(self, Popup::Progress { .. } | Popup::Busy { .. }) {
            return PopupKey::None;
        }
        if key.code == KeyCode::Esc {
            return PopupKey::Close;
        }
        let chord = key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match self {
            Popup::Form(form) => form.key(key),
            Popup::Pick(pick) => pick.key(key),
            Popup::Progress { .. } | Popup::Busy { .. } => PopupKey::None,
            Popup::Confirm { scroll, .. } => match key.code {
                // Only a plain `y` confirms; Ctrl+Y or Alt+Y cancels.
                KeyCode::Char('y') if !chord => PopupKey::Confirmed,
                // The same scroll keys as a message.
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Char('j')
                | KeyCode::Char('k')
                    if !chord =>
                {
                    scroll.key(key.code);
                    PopupKey::None
                }
                _ => PopupKey::Close,
            },
            Popup::Message { scroll, .. } => match key.code {
                KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char(' ') => PopupKey::Close,
                code => {
                    scroll.key(code);
                    PopupKey::None
                }
            },
        }
    }
}

/// Frame and title colour of a popup (TUI-DESIGN.md section 12.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Forms, help, information: `border`.
    Plain,
    /// External actions (upload, mark sent): `warranty`.
    External,
    /// Destructive actions and refusals: `unpaid`.
    Danger,
    /// Results of a write that succeeded: `accent`.
    Done,
}

impl Tone {
    fn color(self, t: &Theme) -> ratatui::style::Color {
        match self {
            Tone::Plain => t.border,
            Tone::External => t.warranty,
            Tone::Danger => t.unpaid,
            Tone::Done => t.accent,
        }
    }
}

/// Dim everything already drawn: fg to `dim`, bold and underline removed,
/// backgrounds kept (section 12.2). Under NO_COLOR only the modifiers go.
pub fn scrim(frame: &mut Frame, theme: &Theme) {
    let buf = frame.buffer_mut();
    let area = buf.area;
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            if !theme.no_color() {
                cell.set_fg(theme.dim);
            }
            let m = cell.modifier - Modifier::BOLD - Modifier::UNDERLINED;
            cell.modifier = m;
        }
    }
}

/// Rows and cells between the frame and the content.
const PAD_Y: u16 = 1;
const PAD_X: u16 = 2;

/// Where a popup of `width` x `height` goes: centred horizontally, its top
/// on the upper third, inside `area` less one row.
pub fn place(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height.saturating_sub(2).max(1));
    let x = area.x + (area.width - width) / 2;
    let y = area.y
        + ((area.height.saturating_sub(height)) / 3)
            .max(1)
            .min(area.height - height);
    Rect::new(x, y, width, height)
}

/// Scrim, clear, frame and fill a popup for `rows` rows of content and
/// return the padded content area. `width` is the outer width.
pub fn open(
    frame: &mut Frame,
    area: Rect,
    width: u16,
    rows: u16,
    title: &str,
    tone: Tone,
    theme: &Theme,
) -> Rect {
    scrim(frame, theme);
    let rect = place(area, width, rows.saturating_add(2 + 2 * PAD_Y));
    frame.render_widget(Clear, rect);
    // A wide glyph cut by the popup edge leaves half a character on screen.
    let buf = frame.buffer_mut();
    for y in rect.top()..rect.bottom() {
        if rect.left() > buf.area.left() {
            let left = &mut buf[(rect.left() - 1, y)];
            if text::width(left.symbol()) > 1 {
                left.set_symbol(" ");
            }
        }
        if rect.right() < buf.area.right() {
            let right = &mut buf[(rect.right(), y)];
            if right.symbol().is_empty() {
                right.set_symbol(" ");
            }
        }
    }
    let color = tone.color(theme);
    let title_style = match tone {
        Tone::Plain => theme.title(),
        _ => theme.title().fg(color),
    };
    let fill = if theme.no_color() {
        Style::new()
    } else {
        Style::new().bg(theme.surface).fg(theme.text)
    };
    // `╭─ title ─╮`: one edge cell before the padded title (section 12.1).
    let border = Style::new().fg(color);
    let max_title = usize::from(rect.width.saturating_sub(7));
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Line::from(vec![
            Span::styled("\u{2500}", border),
            Span::styled(
                format!(" {} ", text::truncate(title, max_title)),
                title_style,
            ),
        ]))
        .style(fill);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    Rect {
        x: inner.x + PAD_X,
        y: inner.y + PAD_Y,
        width: inner.width.saturating_sub(2 * PAD_X),
        height: inner.height.saturating_sub(2 * PAD_Y),
    }
}

/// Outer widths of section 12.1, before clamping to the frame.
const FORM_WIDTH: u16 = 64;
const NOTE_WIDTH: u16 = 56;

/// A key hint line: keys bold `key`, labels `muted`, pairs 2 apart.
fn hint_line(t: &Theme, pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (k, label)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(k.to_string(), t.key()));
        spans.push(Span::styled(format!(" {label}"), t.muted()));
    }
    Line::from(spans)
}

pub fn render(frame: &mut Frame, area: Rect, popup: &Popup, theme: &Theme) {
    let t = theme;
    let (width, tone) = match popup {
        Popup::Form(_) => (FORM_WIDTH, Tone::Plain),
        Popup::Pick(_) => (FORM_WIDTH, Tone::External),
        Popup::Confirm { danger: true, .. } => (NOTE_WIDTH, Tone::Danger),
        Popup::Confirm { .. } => (NOTE_WIDTH, Tone::External),
        Popup::Progress { .. } => (NOTE_WIDTH, Tone::External),
        Popup::Busy { .. } => (NOTE_WIDTH, Tone::Plain),
        Popup::Message { error: true, .. } => (NOTE_WIDTH, Tone::Danger),
        Popup::Message { ok: true, .. } => (NOTE_WIDTH, Tone::Done),
        Popup::Message { .. } => (NOTE_WIDTH, Tone::Plain),
    };
    let width = width.min(area.width.saturating_sub(4)).max(12);
    // Content width inside the frame and the padding.
    let inner_w = usize::from(width.saturating_sub(2 + 4)).max(1);
    let (title, body, footer, scroll): (String, Vec<Line>, Line, Option<&Scroll>) = match popup {
        Popup::Form(form) => (
            form.title.clone(),
            form_lines(form, t, inner_w),
            hint_line(
                t,
                &[
                    ("Tab", "next"),
                    ("Space", "choose"),
                    ("Enter", "submit"),
                    ("Esc", "cancel"),
                ],
            ),
            None,
        ),
        Popup::Pick(pick) => (
            pick.title.clone(),
            pick_lines(pick, t, inner_w),
            hint_line(
                t,
                &[
                    ("\u{2191}\u{2193}", "choose"),
                    ("Enter", "pick"),
                    ("Esc", "cancel"),
                ],
            ),
            None,
        ),
        Popup::Confirm {
            title,
            lines,
            scroll,
            ..
        } => (
            title.clone(),
            confirm_lines(lines, t, inner_w),
            hint_line(t, &[("y", "yes"), ("Esc", "no")]),
            Some(scroll),
        ),
        Popup::Progress {
            title,
            sent,
            total,
            frame: n,
        } => (
            title.clone(),
            progress_lines(*sent, *total, *n, t, inner_w),
            Line::from(Span::styled("one upload at a time, keys wait", t.muted())),
            None,
        ),
        Popup::Busy { title, text } => (
            title.clone(),
            message_lines(std::slice::from_ref(text), false, t, inner_w),
            Line::from(Span::styled("please wait", t.muted())),
            None,
        ),
        Popup::Message {
            title,
            lines,
            error,
            scroll,
            ..
        } => (
            title.clone(),
            message_lines(lines, *error, t, inner_w),
            hint_line(t, &[("Enter", "close")]),
            Some(scroll),
        ),
    };
    // Body, a blank row, the footer.
    let want = (body.len() as u16).saturating_add(2);
    let inner = open(frame, area, width, want, &title, tone, t);
    let footer_y = inner.bottom().saturating_sub(1);
    let body_h = inner.height.saturating_sub(2);
    let hidden = (body.len() as u16).saturating_sub(body_h);
    let offset = match scroll {
        Some(s) => {
            s.max.set(hidden);
            s.offset.min(hidden)
        }
        None => 0,
    };
    let mut footer = footer;
    if hidden > 0 {
        let more = if scroll.is_some() {
            format!(
                "   \u{2191}\u{2193} scroll {}/{}",
                offset + body_h.min(body.len() as u16),
                body.len()
            )
        } else {
            format!("   \u{2193} {hidden} more")
        };
        footer.push_span(Span::styled(more, t.muted()));
    }
    // The form cursor: the real terminal cursor in the focused value.
    if let Popup::Form(form) = popup {
        if let Some((row, col)) = form_cursor(form, inner_w) {
            let y = inner.y + row as u16;
            if y < inner.y + body_h {
                frame.set_cursor_position((inner.x + col as u16, y));
            }
        }
        // The focus band spans the row inside the frame, marker included.
        if let Some(row) = form_focus_row(form) {
            let y = inner.y + row as u16;
            if y < inner.y + body_h && !t.no_color() {
                let buf = frame.buffer_mut();
                let x0 = inner.x.saturating_sub(PAD_X);
                for x in x0..inner.right() + PAD_X {
                    buf[(x, y)].set_bg(t.sel);
                }
            }
        }
    }
    frame.render_widget(
        Paragraph::new(body).scroll((offset, 0)),
        Rect {
            x: inner.x.saturating_sub(PAD_X),
            width: inner.width + PAD_X,
            height: body_h,
            ..inner
        },
    );
    if inner.height > 0 {
        frame.render_widget(
            Paragraph::new(footer),
            Rect::new(inner.x, footer_y, inner.width, 1),
        );
    }
}

/// `label: value` split, for lines that read as facts.
fn fact(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.split_once(": ")?;
    let is_label = !k.is_empty()
        && k.len() <= 16
        && k.chars()
            .all(|c| c.is_ascii_lowercase() || c == ' ' || c == '_');
    is_label.then_some((k, v))
}

/// Rows of text wrapped at `width`, indented `lead` cells (the 2-cell
/// padding that holds the form marker).
fn rows(text: &str, style: Style, width: usize) -> Vec<Line<'static>> {
    text.split('\n')
        .flat_map(|l| text::wrap(l, width))
        .map(|r| Line::from(vec![gap(PAD_X), Span::styled(r, style)]))
        .collect()
}

fn gap(n: u16) -> Span<'static> {
    Span::raw(" ".repeat(usize::from(n)))
}

/// Result and information lines: facts as muted label and text value,
/// links underlined, clipboard notes muted. A refusal's first line (its
/// code) is bold.
fn message_lines(lines: &[String], error: bool, t: &Theme, width: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if error && i == 0 && lines.len() > 1 {
            out.extend(rows(l, t.title().fg(t.unpaid), width));
            continue;
        }
        if error {
            out.extend(rows(l, t.text(), width));
            continue;
        }
        let looks_like_link = !l.contains(' ') && (l.contains("://") || l.starts_with("go."));
        if looks_like_link {
            out.extend(rows(l, t.link(), width));
        } else if l.starts_with("copied to")
            || l.starts_with("clipboard unavailable")
            || l.starts_with("  ")
        {
            out.extend(rows(l, t.muted(), width));
        } else if let Some((k, v)) = fact(l) {
            let lead = format!("{k}  ");
            let mut spans = vec![gap(PAD_X), Span::styled(lead.clone(), t.muted())];
            let first = text::wrap(v, width.saturating_sub(text::width(&lead)));
            let hang = PAD_X as usize + text::width(&lead);
            for (j, r) in first.into_iter().enumerate() {
                if j == 0 {
                    spans.push(Span::styled(r, t.text()));
                    out.push(Line::from(std::mem::take(&mut spans)));
                } else {
                    out.push(Line::from(vec![
                        Span::raw(" ".repeat(hang)),
                        Span::styled(r, t.text()),
                    ]));
                }
            }
        } else {
            out.extend(rows(l, t.text(), width));
        }
    }
    out
}

/// A confirmation: the facts as a label/value block, a blank row, the
/// question in bold, a blank row (section 12.4).
fn confirm_lines(lines: &[String], t: &Theme, width: usize) -> Vec<Line<'static>> {
    let question: Vec<&String> = lines
        .iter()
        .filter(|l| l.trim_end().ends_with('?'))
        .collect();
    let facts: Vec<&String> = lines
        .iter()
        .filter(|l| !l.trim_end().ends_with('?'))
        .collect();
    let label_w = facts
        .iter()
        .filter_map(|l| fact(l))
        .map(|(k, _)| text::width(k))
        .max()
        .unwrap_or(0);
    let mut out = Vec::new();
    for l in &facts {
        match fact(l) {
            Some((k, v)) => {
                let value_w = width.saturating_sub(label_w + 2);
                let mut spans = vec![
                    gap(PAD_X),
                    Span::styled(format!("{k:>label_w$}  "), t.muted()),
                ];
                if k == "warning" {
                    spans.push(Span::styled(
                        text::truncate(v, value_w),
                        t.text().fg(t.warranty),
                    ));
                    out.push(Line::from(spans));
                    for more in text::wrap(v, value_w).into_iter().skip(1) {
                        out.push(Line::from(vec![
                            gap(PAD_X),
                            Span::raw(" ".repeat(label_w + 2)),
                            Span::styled(more, t.text().fg(t.warranty)),
                        ]));
                    }
                    continue;
                }
                if k == "order" {
                    // `delivered -> paid`: the resulting state as its chip.
                    if let Some((from, to)) = v.split_once(" -> ") {
                        use gig_core::models::OrderStatus;
                        let word = |s: &str| {
                            OrderStatus::parse(s)
                                .map_or(s.to_string(), |s| crate::views::status_word(s).to_string())
                        };
                        spans.push(Span::styled(format!("{} -> ", word(from)), t.muted()));
                        let style = OrderStatus::parse(to).map_or(t.text(), |s| t.status(s));
                        spans.push(Span::styled(word(to), style));
                        out.push(Line::from(spans));
                        continue;
                    }
                }
                let shown = if v.contains('/') && !v.contains(' ') {
                    text::truncate_middle(v, value_w, value_w / 2)
                } else {
                    text::truncate(v, value_w)
                };
                spans.push(Span::styled(shown, t.text()));
                out.push(Line::from(spans));
            }
            None => out.extend(rows(l, t.muted(), width)),
        }
    }
    for q in question {
        if !out.is_empty() {
            out.push(Line::raw(""));
        }
        out.extend(rows(q, t.title(), width));
    }
    out
}

/// Label column width in forms.
const LABEL: usize = 14;

/// ASCII hint text shown in an empty, unfocused field.
fn placeholder(label: &str) -> Option<&'static str> {
    Some(match label {
        "slug" => "slug, lowercase-with-dashes",
        "title" => "short human title",
        "price" | "amount" => "e.g. 800",
        "price delta" => "e.g. 200 or -100",
        "date" => "YYYY-MM-DD",
        "material" | "path" => "~/path/to/file",
        "platform" => "xianyu, taobao, ...",
        _ => return None,
    })
}

/// Row of the focused field.
fn form_focus_row(form: &Form) -> Option<usize> {
    let mut row = 0;
    for (i, f) in form.fields.iter().enumerate() {
        if i == form.focus && !f.locked {
            return Some(row);
        }
        row += 1 + usize::from(f.error.is_some());
    }
    None
}

/// Row and column (from the content edge) of the cursor in the focused
/// text field.
fn form_cursor(form: &Form, width: usize) -> Option<(usize, usize)> {
    let row = form_focus_row(form)?;
    let f = form.fields.get(form.focus)?;
    let FieldKind::Text(s) = &f.kind else {
        return None;
    };
    let value_w = width.saturating_sub(LABEL + 2);
    let shown = text::width(&text::tail(s, value_w.saturating_sub(1)));
    Some((row, LABEL + 2 + shown))
}

fn form_lines(form: &Form, t: &Theme, width: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let value_w = width.saturating_sub(LABEL + 2);
    let italic_dim = t.dim().add_modifier(Modifier::ITALIC);
    for (i, f) in form.fields.iter().enumerate() {
        let focused = i == form.focus && !f.locked;
        let label_style = if focused { t.text() } else { t.muted() };
        let value_style = if f.locked { t.muted() } else { t.text() };
        let mut spans = vec![
            if focused {
                Span::styled(crate::views::SELECTED_MARK, t.accent())
            } else {
                Span::raw(" ")
            },
            Span::raw(" "),
            Span::styled(
                format!("{:>LABEL$}", text::truncate(&f.label, LABEL)),
                label_style,
            ),
            Span::raw("  "),
        ];
        match &f.kind {
            FieldKind::Text(s) if s.is_empty() && !focused => {
                if let Some(p) = placeholder(&f.label) {
                    spans.push(Span::styled(p, italic_dim));
                }
            }
            FieldKind::Text(s) => {
                // Long input keeps its end (and the cursor) in view.
                let shown = if focused {
                    text::tail(s, value_w.saturating_sub(1))
                } else {
                    text::truncate(s, value_w)
                };
                spans.push(Span::styled(shown, value_style));
            }
            FieldKind::Select { options, idx } => {
                let v = options.get(*idx).map_or("", String::as_str);
                spans.push(Span::styled("\u{2039} ", t.muted()));
                spans.push(Span::styled(
                    text::truncate(v, value_w.saturating_sub(4)),
                    value_style,
                ));
                spans.push(Span::styled(" \u{203a}", t.muted()));
            }
            FieldKind::Toggle(on) => {
                let (box_, word) = if *on { ("[x]", "yes") } else { ("[ ]", "no") };
                spans.push(Span::styled(box_, t.muted()));
                spans.push(Span::styled(format!(" {word}"), value_style));
            }
            FieldKind::Editor(s) if s.trim().is_empty() => {
                spans.push(Span::styled("Enter opens $EDITOR", italic_dim));
            }
            FieldKind::Editor(s) => {
                let first = s.lines().next().unwrap_or("");
                let more = s.lines().count().saturating_sub(1);
                let tail = if more > 0 {
                    format!("  (+{more} {})", if more == 1 { "line" } else { "lines" })
                } else {
                    String::new()
                };
                spans.push(Span::styled(
                    text::truncate(first, value_w.saturating_sub(text::width(&tail))),
                    value_style,
                ));
                spans.push(Span::styled(tail, t.muted()));
            }
        }
        out.push(Line::from(spans));
        if let Some(e) = &f.error {
            out.push(Line::from(vec![
                Span::raw(" ".repeat(2 + LABEL + 2)),
                Span::styled(text::truncate(&format!("! {e}"), value_w), t.error()),
            ]));
        }
    }
    out
}

fn pick_lines(pick: &Pick, t: &Theme, width: usize) -> Vec<Line<'static>> {
    pick.items
        .iter()
        .enumerate()
        .map(|(i, (_, label))| {
            let shown = text::truncate(label, width);
            if i == pick.selected {
                let pad = width.saturating_sub(text::width(&shown));
                Line::from(vec![
                    Span::styled(crate::views::SELECTED_MARK, t.accent()),
                    Span::raw(" "),
                    Span::styled(shown, t.title()),
                    Span::raw(" ".repeat(pad)),
                ])
                .style(t.selected())
            } else {
                Line::from(vec![gap(PAD_X), Span::styled(shown, t.text())])
            }
        })
        .collect()
}

const SPINNER: [&str; 10] = [
    "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}",
    "\u{2807}", "\u{280f}",
];

/// Partial cells of the progress fill: `▏▎▍▌▋▊▉`.
const PARTS: [&str; 7] = [
    "\u{258f}", "\u{258e}", "\u{258d}", "\u{258c}", "\u{258b}", "\u{258a}", "\u{2589}",
];

/// The fill on its track, the percentage and the sizes; or the spinner
/// while the uploader has not reported (single PUT).
fn progress_lines(
    sent: u64,
    total: u64,
    frame: usize,
    t: &Theme,
    width: usize,
) -> Vec<Line<'static>> {
    let line = if total == 0 {
        Line::from(vec![
            gap(PAD_X),
            Span::styled(SPINNER[frame % SPINNER.len()], t.accent()),
            Span::styled(" uploading", t.muted()),
        ])
    } else {
        let ratio = (sent as f64 / total as f64).clamp(0.0, 1.0);
        let pct = format!("  {:>3}%", (ratio * 100.0).round() as u64);
        let sizes = format!(
            "  {} / {}",
            crate::upload::human_size(sent.min(total)),
            crate::upload::human_size(total)
        );
        let bar = width
            .saturating_sub(text::width(&pct) + text::width(&sizes))
            .clamp(1, 40);
        let eighths = (bar as f64 * 8.0 * ratio).round() as usize;
        let (full, part) = (eighths / 8, eighths % 8);
        let mut fill = "\u{2588}".repeat(full);
        let mut used = full;
        if part > 0 && used < bar {
            fill.push_str(PARTS[part - 1]);
            used += 1;
        }
        Line::from(vec![
            gap(PAD_X),
            Span::styled(fill, t.accent()),
            Span::styled("\u{2500}".repeat(bar - used), t.border()),
            Span::styled(pct, t.text()),
            Span::styled(sizes, t.muted()),
        ])
    };
    vec![line]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn form() -> Form {
        Form::new(
            "t",
            FormKind::NewDraft,
            vec![
                Field::text("slug", "fixed").locked(),
                Field::text("title", ""),
                Field::select("type", &["tool", "custom"], "custom"),
                Field::toggle("flag", false),
                Field::editor("words", ""),
            ],
        )
    }

    #[test]
    fn focus_skips_locked_fields_and_wraps() {
        let mut f = form();
        assert_eq!(f.focus, 1);
        for _ in 0..4 {
            f.key(key(KeyCode::Tab));
        }
        assert_eq!(f.focus, 1, "wrapped past the locked slug");
        f.key(key(KeyCode::BackTab));
        assert_eq!(f.focus, 4);
    }

    #[test]
    fn typing_selecting_toggling() {
        let mut f = form();
        for c in "图 q1".chars() {
            f.key(key(KeyCode::Char(c)));
        }
        f.key(key(KeyCode::Backspace));
        assert_eq!(f.get("title"), "图 q");
        f.key(key(KeyCode::Tab));
        f.key(key(KeyCode::Right));
        assert_eq!(f.get("type"), "tool");
        f.key(key(KeyCode::Left));
        assert_eq!(f.get("type"), "custom");
        f.key(key(KeyCode::Tab));
        f.key(key(KeyCode::Char(' ')));
        assert!(f.field("flag").unwrap().is_on());
        f.key(key(KeyCode::Tab));
        assert_eq!(f.key(key(KeyCode::Enter)), PopupKey::EditField(4));
        f.set_text(4, "a\nb".into());
        assert_eq!(f.get("words"), "a\nb");
        f.key(key(KeyCode::Tab));
        assert_eq!(f.key(key(KeyCode::Enter)), PopupKey::Submit);
        assert_eq!(f.get("slug"), "fixed");
    }

    #[test]
    fn confirm_needs_y() {
        let mut p = Popup::confirm("c", vec![], Effect::None);
        assert_eq!(p.handle_key(key(KeyCode::Char('n'))), PopupKey::Close);
        assert_eq!(p.handle_key(key(KeyCode::Enter)), PopupKey::Close);
        assert_eq!(p.handle_key(key(KeyCode::Char('y'))), PopupKey::Confirmed);
    }

    #[test]
    fn renders_inside_small_frames() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let popups = [
            Popup::Form(form()),
            Popup::confirm("cancel a", vec!["Cancel order a?".into()], Effect::None),
            Popup::message("m", vec!["x".repeat(300)]),
            Popup::Pick(Pick {
                title: "upload a".into(),
                purpose: PickFor::Upload { slug: "a".into() },
                items: vec![("a-v1".into(), "a-v1  full".into())],
                selected: 0,
            }),
            Popup::Progress {
                title: "uploading a-v1".into(),
                sent: 5,
                total: 10,
                frame: 0,
            },
            Popup::Progress {
                title: "uploading a-v1".into(),
                sent: 0,
                total: 0,
                frame: 3,
            },
        ];
        for (w, h) in [(80, 24), (30, 8), (200, 50)] {
            for p in &popups {
                let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
                term.draw(|f| render(f, f.area(), p, &Theme::DARK)).unwrap();
                let buf = term.backend().buffer();
                let text: String = buf.content().iter().map(|c| c.symbol()).collect();
                assert!(text.contains('\u{256d}'), "rounded corner at {w}x{h}");
            }
        }
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        term.draw(|f| render(f, f.area(), &popups[0], &Theme::DARK))
            .unwrap();
        let text: String = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        for label in [
            "slug",
            "title",
            "\u{2039} custom \u{203a}",
            "[ ] no",
            "Enter opens $EDITOR",
            "Esc cancel",
        ] {
            assert!(text.contains(label), "{label}");
        }
    }

    #[test]
    fn long_paths_keep_the_close_hint_visible() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let long = format!("  /home/jc/dev/{}/.gig/JOB.md", "very-long-slug-".repeat(8));
        let p = Popup::message(
            "new order",
            vec!["created files:".into(), long.clone(), long],
        );
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        term.draw(|f| render(f, f.area(), &p, &Theme::DARK))
            .unwrap();
        let text: String = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("close"), "{text}");
    }

    fn screen(p: &Popup, w: u16, h: u16) -> String {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, f.area(), p, &Theme::DARK)).unwrap();
        term.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn long_messages_scroll_and_keep_the_hint() {
        let lines: Vec<String> = (1..=40).map(|i| format!("dirty file {i:02}")).collect();
        let mut p = Popup::message("archive preview", lines);
        let text = screen(&p, 80, 24);
        assert!(text.contains("dirty file 01"));
        assert!(!text.contains("dirty file 40"));
        assert!(text.contains("close"), "hint pinned");
        assert!(text.contains("scroll 16/40"), "{text}");
        // End scrolls to the bottom (max set by the last render).
        assert_eq!(p.handle_key(key(KeyCode::End)), PopupKey::None);
        let text = screen(&p, 80, 24);
        assert!(text.contains("dirty file 40"));
        assert!(!text.contains("dirty file 01"));
        // Scrolling never passes the end; Up comes straight back.
        p.handle_key(key(KeyCode::Down));
        p.handle_key(key(KeyCode::Up));
        let text = screen(&p, 80, 24);
        assert!(!text.contains("dirty file 40") && text.contains("dirty file 39"));
        assert_eq!(p.handle_key(key(KeyCode::Enter)), PopupKey::Close);
    }

    #[test]
    fn long_confirm_keeps_y_hint_and_scrolls() {
        let lines: Vec<String> = (1..=40).map(|i| format!("warning {i}")).collect();
        let mut p = Popup::confirm("upload", lines, Effect::None);
        let text = screen(&p, 80, 24);
        assert!(text.contains("y yes  Esc no"), "{text}");
        assert_eq!(p.handle_key(key(KeyCode::Down)), PopupKey::None);
        let chord = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL);
        assert_eq!(p.handle_key(chord), PopupKey::Close);
        assert_eq!(p.handle_key(key(KeyCode::Char('y'))), PopupKey::Confirmed);
    }

    #[test]
    fn busy_ignores_keys() {
        let mut p = Popup::Busy {
            title: "working".into(),
            text: "checking the package...".into(),
        };
        assert_eq!(p.handle_key(key(KeyCode::Esc)), PopupKey::None);
        assert!(screen(&p, 80, 24).contains("checking the package..."));
    }

    #[test]
    fn pick_moves_and_submits() {
        let mut p = Popup::Pick(Pick {
            title: "t".into(),
            purpose: PickFor::MarkSent { slug: "a".into() },
            items: vec![("x".into(), "x".into()), ("y".into(), "y".into())],
            selected: 0,
        });
        p.handle_key(key(KeyCode::Up));
        p.handle_key(key(KeyCode::Down));
        p.handle_key(key(KeyCode::Down));
        assert_eq!(p.handle_key(key(KeyCode::Enter)), PopupKey::Submit);
        let Popup::Pick(pick) = &p else { panic!() };
        assert_eq!(pick.value(), Some("y"));
        assert_eq!(p.handle_key(key(KeyCode::Esc)), PopupKey::Close);
    }

    #[test]
    fn progress_ignores_keys_and_draws_a_bar_or_spinner() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut p = Popup::Progress {
            title: "uploading a-v1".into(),
            sent: 3 * 1024 * 1024,
            total: 12 * 1024 * 1024,
            frame: 0,
        };
        for code in [KeyCode::Esc, KeyCode::Enter, KeyCode::Char('q')] {
            assert_eq!(p.handle_key(key(code)), PopupKey::None);
        }
        let draw = |p: &Popup| {
            let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
            term.draw(|f| render(f, f.area(), p, &Theme::DARK)).unwrap();
            term.backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
        };
        let text = draw(&p);
        assert!(text.contains("  25%  3.0 MB / 12.0 MB"), "{text}");
        assert!(text.contains('\u{2588}') && text.contains('\u{2500}'));
        let spin = draw(&Popup::Progress {
            title: "uploading a-v1".into(),
            sent: 0,
            total: 0,
            frame: 1,
        });
        assert!(spin.contains("\u{2819} uploading"), "{spin}");
        assert!(!spin.contains('%'));
    }

    #[test]
    fn confirm_colours_scroll_and_message_tones() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let title_fg = |p: &Popup| {
            let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
            term.draw(|f| render(f, f.area(), p, &Theme::DARK)).unwrap();
            let buf = term.backend().buffer().clone();
            // The title is the only capital T on screen.
            let (x, y) = (0..24u16)
                .flat_map(|y| (0..80u16).map(move |x| (x, y)))
                .find(|&(x, y)| buf[(x, y)].symbol() == "T")
                .unwrap();
            assert_eq!(
                buf[(12, y)].symbol(),
                "\u{256d}",
                "56-wide box at 80 columns"
            );
            assert_eq!(buf[(67, y)].symbol(), "\u{256e}");
            buf[(x, y)].fg
        };
        let routine = Popup::confirm("Title", vec!["x".into()], Effect::None);
        let danger = Popup::confirm_danger("Title", vec!["x".into()], Effect::None);
        assert_eq!(title_fg(&routine), Theme::DARK.warranty);
        assert_eq!(title_fg(&danger), Theme::DARK.unpaid);
        assert_eq!(title_fg(&Popup::done("Title", vec![])), Theme::DARK.accent);
        let screen_text = screen(&routine, 80, 24);
        assert!(screen_text.contains("y yes"));
        let mut p = Popup::confirm(
            "c",
            (1..=40).map(|i| format!("w{i}")).collect(),
            Effect::None,
        );
        screen(&p, 80, 24);
        for code in [
            KeyCode::End,
            KeyCode::Home,
            KeyCode::Char('j'),
            KeyCode::Char('k'),
        ] {
            assert_eq!(p.handle_key(key(code)), PopupKey::None, "{code:?}");
        }
        assert_eq!(p.handle_key(key(KeyCode::Char('n'))), PopupKey::Close);
        // A tail opens at its end; Up then moves back one line.
        let mut tail =
            Popup::message_at_end("notes", (1..=40).map(|i| format!("line {i:02}")).collect());
        let text = screen(&tail, 80, 24);
        assert!(
            text.contains("line 40") && !text.contains("line 01"),
            "{text}"
        );
        tail.handle_key(key(KeyCode::Up));
        let text = screen(&tail, 80, 24);
        assert!(
            text.contains("line 39") && !text.contains("line 40"),
            "{text}"
        );
    }

    #[test]
    fn long_titles_stay_inside_the_box() {
        let p = Popup::message("图".repeat(60), vec!["x".into()]);
        let text = screen(&p, 80, 24);
        assert!(text.contains("\u{256e}"), "right corner kept");
        assert!(text.contains('\u{2026}'));
    }
}
