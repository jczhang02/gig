//! Action popups: forms, confirmations, and messages (results and verbatim
//! gig-core refusals). Pure state plus drawing; the gig-core calls and the
//! $EDITOR round trip live in `actions` and `app`.

use crate::actions::{Effect, FormKind};
use crate::text;
use crate::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
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
                match (&mut f.kind, code) {
                    (FieldKind::Text(s), KeyCode::Char(c))
                        if !key.modifiers.contains(KeyModifiers::CONTROL) =>
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
    /// Up/Down/PgUp/PgDn/Home/End; true when the key was a scroll key.
    fn key(&mut self, code: KeyCode) -> bool {
        let max = self.max.get();
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
            scroll: Scroll::default(),
        }
    }

    /// A refusal or failure that is not a gig-core error (editor, missing
    /// file), shown in the error colour.
    pub fn error_text(title: impl Into<String>, text: impl Into<String>) -> Self {
        Popup::Message {
            title: title.into(),
            lines: vec![text.into()],
            error: true,
            scroll: Scroll::default(),
        }
    }

    /// A gig-core refusal, shown verbatim with its code (as the hint line
    /// shows refresh errors).
    pub fn error(e: &gig_core::Error) -> Self {
        Self::error_text("refused", format!("{}: {e}", e.code()))
    }

    pub fn confirm(title: impl Into<String>, lines: Vec<String>, then: Effect) -> Self {
        Popup::Confirm {
            title: title.into(),
            lines,
            then,
            scroll: Scroll::default(),
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
                KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
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

/// Width of the popup box, clamped to the frame.
const WIDTH: u16 = 76;

pub fn render(frame: &mut Frame, area: Rect, popup: &Popup, theme: &Theme) {
    let inner_width = usize::from(WIDTH.min(area.width).saturating_sub(4)).max(1);
    // Body lines are pre-wrapped to `inner_width`, so their count is the
    // exact height; the footer (key hints) is pinned under the body and
    // stays visible however long the body is.
    let (title, title_style, body, footer, scroll): (String, _, Vec<Line>, Vec<Line>, _) =
        match popup {
            Popup::Form(form) => (
                form.title.clone(),
                theme.title(),
                form_lines(form, theme, inner_width),
                vec![form_hint(theme)],
                None,
            ),
            Popup::Confirm {
                title,
                lines,
                scroll,
                ..
            } => (
                title.clone(),
                theme.error(),
                wrapped(lines, theme.text(), inner_width),
                vec![Line::from(vec![
                    Span::styled(" y ", theme.key()),
                    Span::styled("confirm   ", theme.dim()),
                    Span::styled("any other key ", theme.key()),
                    Span::styled("cancel", theme.dim()),
                ])],
                Some(scroll),
            ),
            Popup::Pick(pick) => (
                pick.title.clone(),
                theme.title(),
                pick_lines(pick, theme, inner_width),
                vec![pick_hint(theme)],
                None,
            ),
            Popup::Progress {
                title,
                sent,
                total,
                frame,
            } => (
                title.clone(),
                theme.title(),
                progress_lines(*sent, *total, *frame, theme, inner_width),
                vec![Line::from(Span::styled(
                    "One upload at a time; please wait.",
                    theme.dim(),
                ))],
                None,
            ),
            Popup::Busy { title, text } => (
                title.clone(),
                theme.title(),
                wrapped(std::slice::from_ref(text), theme.text(), inner_width),
                vec![Line::from(Span::styled("please wait", theme.dim()))],
                None,
            ),
            Popup::Message {
                title,
                lines,
                error,
                scroll,
            } => {
                let style = if *error { theme.error() } else { theme.text() };
                let ts = if *error { theme.error() } else { theme.title() };
                (
                    title.clone(),
                    ts,
                    wrapped(lines, style, inner_width),
                    vec![Line::from(vec![
                        Span::styled(" Enter/Esc ", theme.key()),
                        Span::styled("close", theme.dim()),
                    ])],
                    Some(scroll),
                )
            }
        };
    let footer_rows = footer.len() as u16 + 1; // a blank line above the hints
    let want = (body.len() as u16)
        .saturating_add(footer_rows)
        .saturating_add(2);
    let height = want.min(area.height);
    let rect = area.centered(
        Constraint::Length(WIDTH.min(area.width)),
        Constraint::Length(height),
    );
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border())
        .title(Span::styled(format!(" {title} "), title_style))
        .style(theme.base());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    let footer_h = footer_rows.min(inner.height);
    let [body_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(footer_h)]).areas(inner);
    let hidden = (body.len() as u16).saturating_sub(body_area.height);
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
                "  Up/Dn scroll ({}/{})",
                offset + body_area.height.min(body.len() as u16),
                body.len()
            )
        } else {
            format!("  ({hidden} more lines)")
        };
        if let Some(first) = footer.first_mut() {
            first.push_span(Span::styled(more, theme.dim()));
        }
    }
    frame.render_widget(Paragraph::new(body).scroll((offset, 0)), body_area);
    let mut footer_lines = vec![Line::raw("")];
    footer_lines.extend(footer);
    // With only one row left, the hint wins over the blank line.
    let skip = footer_lines
        .len()
        .saturating_sub(usize::from(footer_area.height));
    frame.render_widget(
        Paragraph::new(footer_lines.split_off(skip.min(1))),
        footer_area,
    );
}

/// `lines` hard-wrapped at `width` display cells (Chinese text included);
/// embedded newlines start new rows.
fn wrapped<'a>(lines: &[String], style: Style, width: usize) -> Vec<Line<'a>> {
    lines
        .iter()
        .flat_map(|l| l.split('\n'))
        .flat_map(|l| text::wrap(l, width))
        .map(|row| Line::from(Span::styled(row, style)))
        .collect()
}

/// Label column width in forms.
const LABEL: usize = 14;

fn form_lines<'a>(form: &Form, t: &Theme, width: usize) -> Vec<Line<'a>> {
    let mut out = Vec::new();
    let value_width = width.saturating_sub(LABEL + 2);
    for (i, f) in form.fields.iter().enumerate() {
        let focused = i == form.focus && !f.locked;
        let label_style = if focused { t.key() } else { t.dim() };
        let value_style = if f.locked { t.dim() } else { t.text() };
        let value = match &f.kind {
            FieldKind::Text(s) => {
                if focused {
                    // Keep the end of long input visible.
                    let shown = text::tail(s, value_width.saturating_sub(1));
                    format!("{shown}_")
                } else {
                    text::truncate(s, value_width)
                }
            }
            FieldKind::Select { options, idx } => {
                let v = options.get(*idx).map_or("", String::as_str);
                text::truncate(&format!("< {v} >"), value_width)
            }
            FieldKind::Toggle(on) => (if *on { "[x]" } else { "[ ]" }).to_string(),
            FieldKind::Editor(s) => {
                let first = s.lines().next().unwrap_or("");
                let more = s.lines().count() > 1;
                let shown = if s.trim().is_empty() {
                    "(empty; Enter opens $EDITOR)".to_string()
                } else if more {
                    format!("{first} ...")
                } else {
                    first.to_string()
                };
                text::truncate(&shown, value_width)
            }
        };
        let mut spans = vec![Span::styled(text::fit(&f.label, LABEL), label_style)];
        spans.push(Span::raw("  "));
        let style = if focused {
            value_style.patch(t.selected())
        } else {
            value_style
        };
        spans.push(Span::styled(value, style));
        out.push(Line::from(spans));
    }
    out
}

fn form_hint<'a>(t: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(" Tab ", t.key()),
        Span::styled("next  ", t.dim()),
        Span::styled("Space ", t.key()),
        Span::styled("choose  ", t.dim()),
        Span::styled("Enter ", t.key()),
        Span::styled("submit / edit  ", t.dim()),
        Span::styled("Esc ", t.key()),
        Span::styled("cancel", t.dim()),
    ])
}

fn pick_lines<'a>(pick: &Pick, t: &Theme, width: usize) -> Vec<Line<'a>> {
    let out: Vec<Line> = pick
        .items
        .iter()
        .enumerate()
        .map(|(i, (_, label))| {
            let shown = text::truncate(label, width.saturating_sub(2));
            if i == pick.selected {
                Line::from(vec![
                    Span::styled("> ", t.key()),
                    Span::styled(shown, t.text().patch(t.selected())),
                ])
            } else {
                Line::from(vec![Span::raw("  "), Span::styled(shown, t.text())])
            }
        })
        .collect();
    out
}

fn pick_hint<'a>(t: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(" Up/Dn ", t.key()),
        Span::styled("choose  ", t.dim()),
        Span::styled("Enter ", t.key()),
        Span::styled("pick  ", t.dim()),
        Span::styled("Esc ", t.key()),
        Span::styled("cancel", t.dim()),
    ])
}

const SPINNER: [&str; 10] = [
    "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}",
    "\u{2807}", "\u{280f}",
];

/// A text bar (so it wraps and clips like the other popup lines), or a
/// spinner while the uploader has not reported.
fn progress_lines<'a>(
    sent: u64,
    total: u64,
    frame: usize,
    t: &Theme,
    width: usize,
) -> Vec<Line<'a>> {
    let status = if total == 0 {
        Line::from(vec![
            Span::styled(SPINNER[frame % SPINNER.len()], t.key()),
            Span::styled(" sending", t.text()),
        ])
    } else {
        let ratio = (sent as f64 / total as f64).clamp(0.0, 1.0);
        let label = format!(
            " {:>3}%  {} / {}",
            (ratio * 100.0).round() as u64,
            crate::upload::human_size(sent.min(total)),
            crate::upload::human_size(total)
        );
        let bar = width.saturating_sub(text::width(&label)).clamp(1, 40);
        let filled = ((bar as f64) * ratio).round() as usize;
        Line::from(vec![
            Span::styled("\u{2588}".repeat(filled), t.key()),
            Span::styled("\u{2591}".repeat(bar - filled), t.dim()),
            Span::styled(label, t.text()),
        ])
    };
    vec![status]
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
        for label in ["slug", "title", "< custom >", "[ ]", "$EDITOR", "Esc"] {
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
        assert!(text.contains("Up/Dn scroll"), "{text}");
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
        assert!(text.contains("confirm"), "{text}");
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
        assert!(text.contains(" 25%  3.0 MB / 12.0 MB"), "{text}");
        assert!(text.contains('\u{2588}') && text.contains('\u{2591}'));
        let spin = draw(&Popup::Progress {
            title: "uploading a-v1".into(),
            sent: 0,
            total: 0,
            frame: 1,
        });
        assert!(spin.contains("\u{2819} sending"), "{spin}");
        assert!(!spin.contains('%'));
    }
}
