//! The theme picker (TUI-SPEC section 8.1), opened by `T` and by the
//! `tui.theme` row of Settings: built-ins first, then user files, then the
//! files that failed to load. Moving the cursor previews the theme on the
//! whole screen (`ui::draw` draws with `preview()`); Enter keeps it, Esc
//! restores the one in use.

use crate::mouse::{Hits, Pane, Target};
use crate::popup::{self, Tone};
use crate::text;
use crate::theme::{ColorMode, Theme};
use crate::themes::Catalog;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::cell::Cell;

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    /// The theme as this terminal draws it; `None` for a broken file.
    pub theme: Option<Theme>,
    /// A built-in (not shadowed by a file): `c` can copy it.
    pub builtin: bool,
    /// Why the file did not load.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub rows: Vec<Row>,
    pub cursor: usize,
    /// The theme in use when the picker opened; marked `current`.
    pub current: String,
    /// The colours in use, kept when their file is gone from the catalogue
    /// (deleted while the dashboard runs): listed first as `current`, so
    /// opening the picker previews nothing else.
    pub in_use: Option<Theme>,
    /// First drawn row, kept between frames.
    pub offset: Cell<usize>,
}

/// What a key in the picker asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickCmd {
    None,
    /// Esc: close, the theme in use stays.
    Close,
    /// Enter: write `tui.theme`.
    Keep(String),
    /// `c` on a built-in: copy it to a file and open it in `$EDITOR`.
    Copy(String),
    /// `c` on something that is not a built-in.
    NotBuiltin(String),
}

impl Picker {
    /// Rows from `catalog` drawn in `mode`, the cursor on `current`.
    pub fn new(catalog: &Catalog, current: &str, mode: ColorMode) -> Self {
        let mut rows: Vec<Row> = catalog
            .themes
            .iter()
            .map(|t| Row {
                name: t.name.to_string(),
                builtin: !catalog.user.contains(t.name.as_ref())
                    && Theme::builtin(&t.name).is_some(),
                theme: Some(t.for_mode(mode)),
                error: None,
            })
            .collect();
        rows.extend(catalog.broken.iter().map(|b| Row {
            name: if b.name.is_empty() {
                b.path.display().to_string()
            } else {
                b.name.clone()
            },
            theme: None,
            builtin: false,
            error: Some(b.detail.clone()),
        }));
        let cursor = rows
            .iter()
            .position(|r| r.theme.is_some() && r.name == current)
            .unwrap_or(0);
        Self {
            rows,
            cursor,
            current: current.to_string(),
            in_use: None,
            offset: Cell::new(0),
        }
    }

    /// `theme` is what the screen is drawn with. When the catalogue no
    /// longer lists it (its file was deleted), it gets a row of its own at
    /// the top, with the cursor on it.
    pub fn with_in_use(mut self, theme: &Theme) -> Self {
        let listed = self
            .rows
            .iter()
            .any(|r| r.theme.is_some() && r.name == self.current);
        if !listed {
            self.rows.insert(
                0,
                Row {
                    name: self.current.clone(),
                    theme: Some(theme.clone()),
                    builtin: false,
                    error: None,
                },
            );
            self.cursor = 0;
            self.in_use = Some(theme.clone());
        }
        self
    }

    /// The same picker over a reloaded catalog, the cursor on `name` when it
    /// is still there.
    pub fn rebuilt(&self, catalog: &Catalog, name: &str, mode: ColorMode) -> Self {
        let mut p = Self::new(catalog, &self.current, mode);
        if let Some(t) = &self.in_use {
            p = p.with_in_use(t);
        }
        if let Some(i) = p
            .rows
            .iter()
            .position(|r| r.theme.is_some() && r.name == name)
        {
            p.cursor = i;
        }
        p.offset.set(self.offset.get());
        p
    }

    /// Name under the cursor.
    pub fn highlighted(&self) -> Option<&str> {
        self.rows.get(self.cursor).map(|r| r.name.as_str())
    }

    /// The theme to draw the screen with while the picker is open.
    pub fn preview(&self) -> Option<&Theme> {
        self.rows.get(self.cursor).and_then(|r| r.theme.as_ref())
    }

    /// Selectable rows (broken files are listed, not chosen).
    fn movable(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&i| self.rows[i].theme.is_some())
            .collect()
    }

    pub fn key(&mut self, key: KeyEvent) -> PickCmd {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return PickCmd::None;
        }
        let ok = self.movable();
        let at = ok.iter().position(|&i| i == self.cursor).unwrap_or(0);
        let go = |k: usize| ok.get(k).copied();
        let moved = match key.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => go(at.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                go((at + 1).min(ok.len().saturating_sub(1)))
            }
            KeyCode::Home => go(0),
            KeyCode::End => go(ok.len().saturating_sub(1)),
            KeyCode::PageUp => go(at.saturating_sub(8)),
            KeyCode::PageDown => go((at + 8).min(ok.len().saturating_sub(1))),
            KeyCode::Esc | KeyCode::Char('T') => return PickCmd::Close,
            KeyCode::Enter => {
                return match self.preview() {
                    Some(t) => PickCmd::Keep(t.name.to_string()),
                    None => PickCmd::None,
                }
            }
            KeyCode::Char('c') => {
                return match self.rows.get(self.cursor) {
                    Some(r) if r.builtin => PickCmd::Copy(r.name.clone()),
                    Some(r) => PickCmd::NotBuiltin(r.name.clone()),
                    None => PickCmd::None,
                }
            }
            _ => None,
        };
        if let Some(i) = moved {
            self.cursor = i;
        }
        PickCmd::None
    }
}

/// Outer width of the picker (a form, section 12.1).
const WIDTH: u16 = 64;
/// Cells of the tag column (`current`, `file`).
const TAG_W: usize = 7;

/// The five swatches: bg, text, accent, unpaid, warranty. The bg swatch is
/// `Aa` in the theme's text on its bg: a block in the bg colour would vanish
/// on a popup fill of the same lightness.
fn swatches(t: &Theme) -> Vec<Span<'static>> {
    let paint = |c: Color| {
        if c == Color::Reset {
            Style::new()
        } else {
            Style::new().fg(c)
        }
    };
    let mut bg = paint(t.text);
    if t.bg != Color::Reset {
        bg = bg.bg(t.bg);
    }
    let mut out = vec![Span::styled("Aa", bg)];
    for c in [t.text, t.accent, t.unpaid, t.warranty] {
        out.push(Span::raw(" "));
        out.push(Span::styled("\u{2588}\u{2588}", paint(c)));
    }
    out
}

pub fn render(frame: &mut Frame, area: Rect, p: &Picker, t: &Theme, hits: &Hits) {
    let width = WIDTH.min(area.width.saturating_sub(4)).max(20);
    // Content width inside the frame and padding; lines start in the
    // padding so the marker sits in its first cell.
    let inner_w = usize::from(width.saturating_sub(6));
    let name_w = p
        .rows
        .iter()
        .map(|r| text::width(&r.name))
        .max()
        .unwrap_or(0)
        .clamp(8, 20)
        .min(inner_w.saturating_sub(TAG_W + 2));
    let sw_x = name_w + 2 + TAG_W + 2;
    let show_swatches = inner_w >= sw_x + 14;

    let mut body = Vec::new();
    for (i, r) in p.rows.iter().enumerate() {
        let selected = i == p.cursor;
        let mut spans = vec![
            if selected {
                Span::styled(crate::views::SELECTED_MARK, t.accent())
            } else {
                Span::raw(" ")
            },
            Span::raw(" "),
        ];
        match &r.theme {
            Some(theme) => {
                let style = if selected { t.title() } else { t.text() };
                spans.push(Span::styled(text::fit(&r.name, name_w), style));
                spans.push(Span::raw("  "));
                let (tag, style) = if r.name == p.current {
                    ("current", t.accent())
                } else if !r.builtin {
                    ("file", t.muted())
                } else {
                    ("", t.muted())
                };
                spans.push(Span::styled(text::fit(tag, TAG_W), style));
                if show_swatches {
                    spans.push(Span::raw("  "));
                    spans.extend(swatches(theme));
                }
            }
            None => {
                // A broken file: `muted` ink instead of `text`, and the
                // reason (the dim slot never carries a fact alone).
                spans.push(Span::styled(text::fit(&r.name, name_w), t.muted()));
                spans.push(Span::raw("  "));
                let err = format!("! {}", r.error.as_deref().unwrap_or(""));
                spans.push(Span::styled(
                    text::truncate(&err, inner_w.saturating_sub(name_w + 2)),
                    t.muted(),
                ));
            }
        }
        body.push(Line::from(spans));
    }
    let footer = popup::hint_line(
        t,
        &[
            ("\u{2191}\u{2193}", "preview"),
            ("Enter", "keep"),
            ("c", "copy"),
            ("Esc", "restore"),
        ],
    );
    let want = (body.len() as u16).saturating_add(2);
    let inner = popup::open(frame, area, width, want, "theme", Tone::Plain, t, hits);
    if let Some(m) = hits.map().modal {
        hits.pane(m, Pane::Picker);
    }
    let body_h = usize::from(inner.height.saturating_sub(2));
    // Keep the cursor row in view.
    let mut offset = p.offset.get().min(body.len().saturating_sub(body_h));
    if p.cursor < offset {
        offset = p.cursor;
    } else if body_h > 0 && p.cursor >= offset + body_h {
        offset = p.cursor + 1 - body_h;
    }
    p.offset.set(offset);
    let x0 = inner.x.saturating_sub(popup::PAD_X);
    let full_w = inner.width + 2 * popup::PAD_X;
    let cursor_y = p.cursor.checked_sub(offset).filter(|&k| k < body_h);
    if let (Some(k), false) = (cursor_y, t.no_color()) {
        let rect = Rect::new(x0, inner.y + k as u16, full_w, 1);
        frame.buffer_mut().set_style(rect, t.selected());
    }
    frame.render_widget(
        Paragraph::new(body.clone()).scroll((offset as u16, 0)),
        Rect::new(x0, inner.y, inner.width + popup::PAD_X, body_h as u16),
    );
    // Mouse: every drawn row that holds a theme (broken files are listed,
    // not chosen).
    for (k, i) in (offset..p.rows.len()).take(body_h).enumerate() {
        if p.rows[i].theme.is_some() {
            hits.add(
                Rect::new(x0, inner.y + k as u16, full_w, 1),
                Target::Theme(i),
            );
        }
    }
    let mut footer = footer;
    let hidden = body.len().saturating_sub(body_h);
    if hidden > 0 {
        footer.push_span(Span::styled(
            format!("   {}/{}", (offset + body_h).min(body.len()), body.len()),
            t.muted(),
        ));
    }
    if inner.height >= 1 {
        hits.hints(inner.x, inner.bottom() - 1, &footer, inner.right());
        frame.render_widget(
            Paragraph::new(footer),
            Rect {
                y: inner.bottom() - 1,
                height: 1,
                ..inner
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::Broken;

    fn catalog() -> Catalog {
        let mut c = Catalog::load(std::path::Path::new("/nonexistent"));
        let mut user = Theme::NORD.clone();
        user.name = "zz-mine".into();
        c.themes.push(user);
        c.user.insert("zz-mine".into());
        c.broken.push(Broken {
            path: "/t/bad.toml".into(),
            name: "bad".into(),
            detail: "missing key \"bar\"".into(),
        });
        c
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn rows_cursor_and_keys() {
        let mut p = Picker::new(&catalog(), "nord", ColorMode::TrueColor);
        assert_eq!(p.rows.len(), 10);
        assert_eq!(p.highlighted(), Some("nord"));
        assert!(p.rows[0].builtin && !p.rows[8].builtin);
        assert_eq!(p.preview().unwrap().name, "nord");
        p.key(key(KeyCode::Down));
        assert_eq!(p.preview().unwrap().name, "dracula");
        p.key(key(KeyCode::End));
        // The broken row is not selectable.
        assert_eq!(p.highlighted(), Some("zz-mine"));
        assert_eq!(
            p.key(key(KeyCode::Char('c'))),
            PickCmd::NotBuiltin("zz-mine".into())
        );
        p.key(key(KeyCode::Down));
        assert_eq!(p.highlighted(), Some("zz-mine"));
        p.key(key(KeyCode::Home));
        assert_eq!(
            p.key(key(KeyCode::Char('c'))),
            PickCmd::Copy("gig-dark".into())
        );
        assert_eq!(p.key(key(KeyCode::Enter)), PickCmd::Keep("gig-dark".into()));
        assert_eq!(p.key(key(KeyCode::Esc)), PickCmd::Close);
    }

    #[test]
    fn a_theme_whose_file_is_gone_stays_current() {
        let mut gone = Theme::NORD.clone();
        gone.name = "dracula-copy".into();
        let p = Picker::new(&catalog(), "dracula-copy", ColorMode::TrueColor).with_in_use(&gone);
        assert_eq!(p.cursor, 0);
        assert_eq!(p.rows[0].name, "dracula-copy");
        assert_eq!(p.preview(), Some(&gone), "opening previews nothing else");
        // Listed themes get no extra row.
        let p = Picker::new(&catalog(), "nord", ColorMode::TrueColor).with_in_use(&Theme::NORD);
        assert_eq!(p.rows.iter().filter(|r| r.name == "nord").count(), 1);
        assert_eq!(p.in_use, None);
        // A rebuild keeps the row.
        let p = Picker::new(&catalog(), "dracula-copy", ColorMode::TrueColor).with_in_use(&gone);
        let r = p.rebuilt(&catalog(), "dracula-copy", ColorMode::TrueColor);
        assert_eq!(r.rows[0].name, "dracula-copy");
    }
}
