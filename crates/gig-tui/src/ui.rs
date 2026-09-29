//! The frame (TUI-DESIGN.md sections 6 and 7): banner on row 0, the
//! message row, the view body, and the key hints on the last row.
//! Whitespace and colour instead of borders.

use crate::app::{Tone, UiState, View};
use crate::icons::Icons;
use crate::theme::Theme;
use crate::{help, popup, text, views};
use ratatui::buffer::CellWidth;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

/// Borrowed bundle passed to every render function.
pub struct RenderCx<'a> {
    pub state: &'a UiState,
    pub theme: &'a Theme,
    pub icons: &'a Icons,
}

/// Width classes of section 6.2; every view and the key handling use them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WidthClass {
    /// Under 110 columns: lists full width, `Enter` opens the detail.
    Narrow,
    /// 110 to 159: list and detail pane.
    Medium,
    /// 160 and wider: the `next` column comes in.
    Wide,
}

impl WidthClass {
    pub fn of(width: u16) -> Self {
        match width {
            0..110 => WidthClass::Narrow,
            110..160 => WidthClass::Medium,
            _ => WidthClass::Wide,
        }
    }
}

/// Smallest frame the dashboard draws into.
pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 16;

/// Orders cells before the title column, gap included (section 6.3).
const FIXED: i32 = 57;
/// The same with the Wide `next` column.
const FIXED_WIDE: i32 = 72;
/// Narrowest title column worth drawing.
pub const MIN_TITLE: usize = 10;

/// List and pane widths at terminal width `width` (section 6.3 arithmetic;
/// margins 1 + 1, gutter 2). The pane is 0 at Narrow.
pub fn split(width: u16) -> (u16, u16) {
    let w = i32::from(width);
    let (l, p) = match WidthClass::of(width) {
        WidthClass::Narrow => (w - 2, 0),
        WidthClass::Medium => {
            let p = (w - 4 - FIXED - 20).clamp(40, 64);
            let l = w - 4 - p;
            if l - FIXED < MIN_TITLE as i32 {
                // No room for a title: the list keeps its columns and the
                // pane takes the rest.
                (FIXED - 2, w - 4 - (FIXED - 2))
            } else {
                (l, p)
            }
        }
        WidthClass::Wide => {
            let title = (w - 4 - FIXED_WIDE - 2 - 60).clamp(24, 40);
            let l = FIXED_WIDE + 2 + title;
            (l, w - 4 - l)
        }
    };
    (l.max(0) as u16, p.max(0) as u16)
}

/// Areas of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shell {
    pub banner: Rect,
    /// Row 1: filter on the left, toast on the right.
    pub message: Rect,
    /// The view body (the list when a pane is open), inside the margins.
    pub body: Rect,
    /// The right pane: Orders detail, or the Drafts notes tail.
    pub pane: Option<Rect>,
    pub footer: Rect,
}

pub fn shell(area: Rect, state: &UiState) -> Shell {
    let row = |y: u16| Rect::new(area.x, area.y + y, area.width, 1);
    let inner_w = area.width.saturating_sub(2);
    let body_h = area.height.saturating_sub(3);
    let class = WidthClass::of(area.width);
    let has_pane = class != WidthClass::Narrow
        && !state.detail_open
        && state.settings.is_none()
        && match state.view {
            View::Orders => true,
            View::Drafts => state.notes_pane.is_some(),
            _ => false,
        };
    let (body, pane) = if state.detail_open && state.settings.is_none() {
        // Full-screen detail: left margin 2, prose measure 76.
        let w = area.width.saturating_sub(4).min(76);
        (Rect::new(area.x + 2, area.y + 2, w, body_h), None)
    } else if has_pane {
        let (l, p) = split(area.width);
        (
            Rect::new(area.x + 1, area.y + 2, l, body_h),
            Some(Rect::new(area.x + 1 + l + 2, area.y + 2, p, body_h)),
        )
    } else {
        (Rect::new(area.x + 1, area.y + 2, inner_w, body_h), None)
    };
    Shell {
        banner: row(0),
        message: row(1),
        body,
        pane,
        footer: row(area.height.saturating_sub(1)),
    }
}

pub fn draw(frame: &mut Frame, cx: &RenderCx) {
    // The theme picker previews the highlighted theme on the whole screen.
    if let Some(preview) = cx.state.picker.as_ref().and_then(|p| p.preview()) {
        if preview != cx.theme {
            let cx = RenderCx {
                state: cx.state,
                theme: preview,
                icons: cx.icons,
            };
            return draw_frame(frame, &cx);
        }
    }
    draw_frame(frame, cx)
}

fn draw_frame(frame: &mut Frame, cx: &RenderCx) {
    let area = frame.area();
    let t = cx.theme;
    if !t.no_color() {
        frame.render_widget(Block::new().style(t.base()), area);
    }
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let msg = format!(
            "gig needs {MIN_WIDTH}x{MIN_HEIGHT}, this is {}x{}",
            area.width, area.height
        );
        let y = area.y + area.height / 2;
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(&msg, usize::from(area.width)),
                t.muted(),
            )))
            .centered(),
            Rect::new(area.x, y, area.width, 1),
        );
        return;
    }
    let s = shell(area, cx.state);
    draw_banner(frame, s.banner, cx);
    draw_message(frame, s.message, cx);
    if let Some(settings) = &cx.state.settings {
        crate::settings::render(frame, s.body, cx, settings);
    } else if cx.state.detail_open {
        views::detail::render(frame, s.body, cx, true);
    } else {
        views::render(frame, s.body, cx.state.view, cx);
    }
    if let Some(pane) = s.pane {
        match cx.state.view {
            View::Drafts => views::drafts::render_notes(frame, pane, cx),
            _ => views::detail::render(frame, pane, cx, false),
        }
    }
    draw_footer(frame, s.footer, cx);
    if cx.state.help_open {
        help::render(frame, area, cx);
    }
    if let Some(p) = &cx.state.picker {
        crate::picker::render(frame, area, p, cx.theme);
    }
    if let Some(p) = &cx.state.popup {
        popup::render(frame, area, p, cx.theme);
    }
    settle_wide_glyphs(frame.buffer_mut(), t);
}

/// Last pass over the frame (section 14.4: bg on every cell of every frame).
///
/// The cells a wide glyph covers are reset to the default cell. Paragraph
/// leaves them holding the style painted underneath, so after a redraw that
/// moves CJK text one cell to the left the previous and next frame agree on
/// such a cell and ratatui's diff skips it, while the terminal has already
/// cleared it (it was the orphaned half of the old glyph) to its own default
/// background: a hole in the page. With every covered cell at its default in
/// every frame, a covered cell that becomes visible always differs and is
/// redrawn. Any other cell still on the default background gets `bg`.
fn settle_wide_glyphs(buf: &mut ratatui::buffer::Buffer, t: &Theme) {
    let area = buf.area;
    for y in area.top()..area.bottom() {
        let mut x = area.left();
        while x < area.right() {
            let cell = &mut buf[(x, y)];
            if !t.no_color() && cell.bg == ratatui::style::Color::Reset {
                cell.bg = t.bg;
            }
            // The width ratatui's diff uses, so both agree on what is covered.
            let w = cell.cell_width().max(1);
            for k in x + 1..x.saturating_add(w).min(area.right()) {
                buf[(k, y)].reset();
            }
            x = x.saturating_add(w);
        }
    }
}

/// Lower-case three-letter English month of `YYYY-MM-DD`.
pub fn month_abbr(date: &str) -> &'static str {
    const M: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    date.get(5..7)
        .and_then(|m| m.parse::<usize>().ok())
        .and_then(|m| M.get(m.wrapping_sub(1)))
        .copied()
        .unwrap_or("month")
}

/// `gig`, the tabs, and the money cluster on the right (section 7.1).
fn draw_banner(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let width = usize::from(area.width);
    let gap = if area.width < 100 { "  " } else { "   " };
    let tabs = |compact: bool| -> Line<'static> {
        let mut spans = vec![Span::raw(" "), Span::styled("gig", t.title())];
        for v in View::ALL {
            spans.push(Span::raw(gap));
            let active = v == cx.state.view;
            spans.push(Span::styled(format!("{}", v.index() + 1), t.muted()));
            if active {
                let mut style = t.title().add_modifier(Modifier::UNDERLINED);
                if !t.no_color() {
                    style = style.underline_color(t.accent);
                }
                spans.push(Span::raw(" "));
                spans.push(Span::styled(v.title(), style));
            } else if !compact {
                spans.push(Span::styled(format!(" {}", v.title()), t.muted()));
            }
        }
        Line::from(spans)
    };
    let m = &cx.state.data.money;
    let today = &cx.state.data.today;
    let owed_style = if m.outstanding.gross > 0 {
        t.title().fg(t.unpaid)
    } else {
        t.text()
    };
    let items: [(String, String, ratatui::style::Style); 3] = [
        ("owed".into(), text::money(m.outstanding.gross), owed_style),
        (
            month_abbr(today).into(),
            text::money(m.month.gross),
            t.text(),
        ),
        (
            today.get(..4).unwrap_or("year").into(),
            text::money(m.year.gross),
            t.text(),
        ),
    ];
    let cluster = |n: usize, currency: bool| -> Line<'static> {
        let mut spans = Vec::new();
        for (i, (label, value, style)) in items.iter().take(n).enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
                spans.push(Span::styled("\u{b7}", t.dim()));
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(format!("{label} "), t.muted()));
            spans.push(Span::styled(value.clone(), *style));
        }
        if currency {
            spans.push(Span::styled(
                format!(" {}", cx.state.data.currency()),
                t.muted(),
            ));
        }
        spans.push(Span::raw(" "));
        Line::from(spans)
    };
    // Degradation: CNY, then the year, then the month; owed goes last,
    // and then the tabs lose their inactive words.
    let tries = [(3, true), (3, false), (2, false), (1, false)];
    for compact in [false, true] {
        let left = tabs(compact);
        for (n, currency) in tries {
            let right = cluster(n, currency);
            if left.width() + 3 + right.width() <= width {
                frame.render_widget(Paragraph::new(left), area);
                frame.render_widget(Paragraph::new(right.right_aligned()), area);
                return;
            }
        }
    }
    frame.render_widget(Paragraph::new(tabs(true)), area);
}

/// Row 1: the filter on the left, one toast (or the refresh error) on the
/// right.
fn draw_message(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let width = usize::from(area.width);
    let toast = cx
        .state
        .toast
        .as_ref()
        .map(|toast| {
            let style = match toast.tone {
                Tone::Info => t.text(),
                Tone::Warn => t.text().fg(t.warranty),
                Tone::Error => t.error(),
            };
            (toast.text.clone(), style)
        })
        .or_else(|| cx.state.error.clone().map(|e| (e, t.error())));
    let mut right_w = 0;
    if let Some((msg, style)) = toast {
        let msg = text::truncate(&msg, width.saturating_sub(2) * 2 / 3);
        right_w = text::width(&msg) + 1;
        frame.render_widget(
            Paragraph::new(
                Line::from(vec![Span::styled(msg, style), Span::raw(" ")]).right_aligned(),
            ),
            area,
        );
    }
    let filter = cx.state.filter();
    if filter.editing || !filter.text.is_empty() {
        let count = views::filter_count(cx.state);
        let room = width.saturating_sub(right_w + 2 + 3 + text::width(&count) + 2);
        let mut spans = vec![
            Span::raw(" "),
            Span::styled(format!("/ {}", text::tail(&filter.text, room)), t.accent()),
        ];
        if filter.editing {
            spans.push(Span::styled("\u{258f}", t.accent()));
        }
        spans.push(Span::styled(format!("  {count}"), t.muted()));
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

/// The contextual key hints (section 7.3).
fn draw_footer(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let line = match cx.state.popup {
        Some(popup::Popup::Progress { .. }) => Line::from(Span::styled(
            " uploading; keys are ignored until it ends",
            t.muted(),
        )),
        Some(popup::Popup::Busy { .. }) => Line::from(Span::styled(
            " working; keys are ignored until it ends",
            t.muted(),
        )),
        _ => {
            let class = WidthClass::of(area.width);
            let (g1, g2) = help::footer_groups(cx.state, class);
            let (g1, g2) = help::fit_footer(g1, g2, usize::from(area.width.saturating_sub(2)));
            let mut line = help::footer_line(cx, &g1, &g2);
            line.spans.insert(0, Span::raw(" "));
            line
        }
    };
    frame.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::View;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    fn render(width: u16, height: u16, state: &UiState, icons: bool) -> Buffer {
        render_with(width, height, state, icons, Theme::DARK)
    }

    fn render_with(width: u16, height: u16, state: &UiState, icons: bool, theme: Theme) -> Buffer {
        let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
        let icons = Icons::new(icons);
        term.draw(|f| {
            draw(
                f,
                &RenderCx {
                    state,
                    theme: &theme,
                    icons: &icons,
                },
            )
        })
        .unwrap();
        term.backend().buffer().clone()
    }

    /// Row `y` as text; the cell after a wide character is skipped.
    fn row(buf: &Buffer, y: u16) -> String {
        span_text(buf, y, 0..buf.area.width)
    }

    fn span_text(buf: &Buffer, y: u16, xs: std::ops::Range<u16>) -> String {
        let mut out = String::new();
        let mut x = xs.start;
        while x < xs.end {
            let sym = buf[(x, y)].symbol();
            out.push_str(sym);
            x += (crate::text::width(sym) as u16).max(1);
        }
        out
    }

    fn all(buf: &Buffer) -> String {
        (0..buf.area.height)
            .map(|y| row(buf, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn shell_at_80x24_hides_detail() {
        let state = UiState::default();
        let buf = render(80, 24, &state, false);
        assert_eq!(buf.area, Rect::new(0, 0, 80, 24));
        let top = row(&buf, 0);
        for v in View::ALL {
            assert!(top.contains(v.title()), "banner lacks {}: {top}", v.title());
        }
        assert!(row(&buf, 23).contains("q quit"));
        let text = all(&buf);
        assert!(text.contains("nothing needs you"), "{text}");
        assert!(text.contains("N new order"), "{text}");
        assert!(!text.contains("no order selected"), "no detail pane");
        assert!(shell(buf.area, &state).pane.is_none());
    }

    #[test]
    fn shell_at_200x50_shows_detail_and_money() {
        let state = UiState::default();
        let buf = render(200, 50, &state, true);
        let top = row(&buf, 0);
        assert!(top.contains("Orders"));
        assert!(top.contains("owed"));
        assert!(row(&buf, 49).contains("q quit"));
        let s = shell(buf.area, &state);
        let detail = s.pane.expect("detail pane at 200 columns");
        assert!(detail.x > s.body.x + s.body.width);
        // Beside an empty list the pane is blank.
        assert!(!all(&buf).contains("no order selected"));
        // Pane layout stays inside the frame.
        assert!(detail.right() <= 200 && s.footer.bottom() <= 50);
    }

    #[test]
    fn banner_shows_snapshot_money() {
        let mut state = UiState::default();
        state.data.money.outstanding.gross = 130000;
        state.data.money.month.gross = 80050;
        state.data.money.year.gross = 1_200_000;
        state.data.today = "2026-09-29".into();
        let buf = render(200, 50, &state, true);
        let top = row(&buf, 0);
        assert!(
            top.ends_with("owed 1,300  \u{b7}  sep 800.50  \u{b7}  2026 12,000 CNY "),
            "{top}"
        );
        assert!(top.starts_with(" gig   1 Orders   2 Drafts   3 Money   4 History"));
        // The active tab: bold text with an accent underline.
        let x = top.find("Orders").unwrap() as u16;
        let c = &buf[(x, 0)];
        assert!(c.modifier.contains(Modifier::BOLD | Modifier::UNDERLINED));
        assert_eq!(c.underline_color, Theme::DARK.accent);
        // Owed > 0 is bold unpaid; 0 is plain text.
        let x = top.find("1,300").unwrap() as u16;
        assert_eq!(buf[(x, 0)].fg, Theme::DARK.unpaid);
        state.data.money.outstanding.gross = 0;
        let buf = render(200, 50, &state, true);
        let x = row(&buf, 0).find("owed 0").unwrap() as u16 + 5;
        assert_eq!(buf[(x, 0)].fg, Theme::DARK.text);
    }

    #[test]
    fn below_min_size_only_the_notice() {
        let buf = render(59, 16, &sample(), true);
        let text: Vec<String> = (0..16)
            .map(|y| row(&buf, y).trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(text, vec!["gig needs 60x16, this is 59x16"]);
        assert!(!all(&render(60, 16, &sample(), true)).contains("gig needs"));
    }

    #[test]
    fn width_classes_and_split() {
        assert_eq!(WidthClass::of(109), WidthClass::Narrow);
        assert_eq!(WidthClass::of(110), WidthClass::Medium);
        assert_eq!(WidthClass::of(160), WidthClass::Wide);
        assert_eq!(split(80), (78, 0));
        assert_eq!(split(110), (55, 51));
        assert_eq!(split(120), (76, 40));
        assert_eq!(split(139), (77, 58));
        assert_eq!(split(159), (91, 64));
        assert_eq!(split(160), (98, 58));
        assert_eq!(split(200), (114, 82));
        for w in 60..300 {
            let (l, p) = split(w);
            if p > 0 {
                assert_eq!(l + p, w - 4, "{w}");
                assert!(p >= 40, "{w}");
            }
        }
    }

    #[test]
    fn toasts_show_on_the_message_row() {
        let mut state = UiState {
            toast: Some(crate::app::Toast::info("copied go.jczhang.cc/a30bd870")),
            ..UiState::default()
        };
        let buf = render(80, 24, &state, false);
        assert!(row(&buf, 1).ends_with("copied go.jczhang.cc/a30bd870 "));
        state.toast = Some(crate::app::Toast::warn(
            "unknown theme \"nrod\", using gig-dark",
        ));
        let buf = render(80, 24, &state, false);
        let r = row(&buf, 1);
        let x = crate::text::width(&r[..r.find("unknown").unwrap()]) as u16;
        assert_eq!(buf[(x, 1)].fg, Theme::DARK.warranty);
        // Any key clears it.
        state.handle_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Down,
                crossterm::event::KeyModifiers::NONE,
            ),
            80,
        );
        assert!(state.toast.is_none());
    }

    #[test]
    fn other_views_are_full_width() {
        for (v, head) in [
            (View::Drafts, "draft"),
            (View::Money, "outstanding"),
            (View::History, "warranty"),
        ] {
            let state = UiState {
                view: v,
                ..UiState::default()
            };
            let buf = render(200, 50, &state, true);
            assert!(shell(buf.area, &state).pane.is_none());
            assert!(row(&buf, 2).contains(head), "{v:?}: {}", row(&buf, 2));
        }
    }

    #[test]
    fn empty_list_leaves_the_pane_blank() {
        for w in [120, 200] {
            let text = all(&render(w, 36, &UiState::default(), true));
            assert!(text.contains("nothing needs you"), "{text}");
            assert!(!text.contains("no order selected"), "{text}");
        }
    }

    #[test]
    fn closed_orders_have_an_empty_next_cell() {
        let mut state = sample();
        state.show_closed = true;
        let buf = render(200, 50, &state, true);
        let text = all(&buf);
        for slug in ["o8", "o9"] {
            let (y, line) = line_of(&text, &format!(" {slug} ")).unwrap();
            assert!(!line.contains("none"), "{line}");
            let x = crate::text::width(&line[..line.find("\u{b7}").unwrap()]) as u16;
            assert_eq!(buf[(x, y as u16)].fg, Theme::DARK.dim, "{line}");
        }
        assert!(text.contains("archive"), "open rows keep their next step");
    }

    #[test]
    fn narrow_enter_shows_detail_full_screen() {
        let state = UiState {
            detail_open: true,
            ..UiState::default()
        };
        let buf = render(80, 24, &state, false);
        assert!(row(&buf, 2).contains("no order selected"));
        assert!(row(&buf, 23).contains("Esc back"));
    }

    #[test]
    fn help_popup_lists_keys() {
        for (w, h) in [(80, 24), (200, 50)] {
            let state = UiState {
                help_open: true,
                ..UiState::default()
            };
            let text = all(&render(w, h, &state, true));
            for s in [
                "keys",
                "refresh",
                "close, clear",
                "theme picker",
                "settings",
                "mark sent",
                "when a package is checked",
                "theme gig-dark",
                "\u{256d}\u{2500} keys \u{2500}",
            ] {
                assert!(text.contains(s), "{w}x{h}: {s}\n{text}");
            }
        }
        // Mockup 17.4: a 4-cell key column; `Enter` reaches into the padding.
        let state = UiState {
            help_open: true,
            ..UiState::default()
        };
        let text = all(&render(120, 36, &state, true));
        for s in [
            "\u{2502}  global                orders",
            "\u{2502}     ?  keys               s  start      when queued or delivered",
            "\u{2502} Enter  open               U  upload artifact",
        ] {
            assert!(text.contains(s), "{s}\n{text}");
        }
        // Too short for every key: both columns scroll, with markers.
        let mut state = state;
        let text = all(&render(60, 16, &state, true));
        assert!(text.contains("\u{2193} 6 more"), "{text}");
        assert!(!text.contains("above"), "{text}");
        state.help_scroll.offset = 99;
        let text = all(&render(60, 16, &state, true));
        assert!(text.contains("\u{2191} 9 above"), "{text}");
        assert!(text.contains("PgDn  scroll detail"), "{text}");
        assert!(!text.contains("more"), "{text}");
        // Money has 3 rows of keys: its column says 3, not 9.
        state.view = View::Money;
        let text = all(&render(60, 16, &state, true));
        assert!(text.contains("\u{2191} 3 above"), "{text}");
    }

    #[test]
    fn filter_input_shows_on_hint_line() {
        let mut state = UiState::default();
        state.filters[0].editing = true;
        state.filters[0].text = "tk-".into();
        let buf = render(80, 24, &state, false);
        assert!(
            row(&buf, 1).starts_with(" / tk-\u{258f}  0 of 0"),
            "{}",
            row(&buf, 1)
        );
        assert_eq!(buf[(1, 1)].fg, Theme::DARK.accent);
    }

    #[test]
    fn busy_popup_hint_says_keys_are_ignored() {
        let state = UiState {
            popup: Some(popup::Popup::Busy {
                title: "working".into(),
                text: "checking the package...".into(),
            }),
            ..UiState::default()
        };
        let buf = render(80, 24, &state, false);
        assert!(row(&buf, 23).contains("keys are ignored"));
    }

    #[test]
    fn errors_show_on_hint_line() {
        let state = UiState {
            error: Some("db: locked".into()),
            ..UiState::default()
        };
        let buf = render(80, 24, &state, false);
        assert!(row(&buf, 1).ends_with("db: locked "));
    }

    /// Sample orders with a long Chinese title, a JOB.md status on the
    /// selected row, a package, a note and a scorecard.
    fn sample() -> UiState {
        use crate::data::tests::sample_snapshot;
        use crate::data::JobMd;
        let mut data = sample_snapshot("2026-09-29");
        for r in &mut data.orders {
            if r.order.id == 7 {
                r.order.title =
                    "图像去噪与超分辨率批处理工具开发及交付, 含批量脚本, 使用说明和演示视频".into();
                r.order.slug = "tk-denoise".into();
                r.job = JobMd::parse("## Status\n\n- 2026-09-20: 预览已发送, 等客户确认.\n");
                r.notes = crate::data::parse_notes("[2026-09-21T10:00:00Z] client paid half\n");
                r.scorecard = Some(gig_core::models::Scorecard {
                    order_id: 7,
                    decisions: Some(3),
                    repeat_questions: Some(0),
                    days_to_preview: None,
                    cleanups: Some(1),
                    check_rejections: None,
                    report_reworks: Some(0),
                    score: Some(4),
                    note: None,
                    created_at: "2026-09-22T00:00:00Z".into(),
                });
            }
        }
        UiState {
            data,
            selected: Some(7),
            ..UiState::default()
        }
    }

    fn line_of<'a>(text: &'a str, needle: &str) -> Option<(usize, &'a str)> {
        text.lines().enumerate().find(|(_, l)| l.contains(needle))
    }

    #[test]
    fn orders_rows_at_80x24_120x36_and_200x50() {
        let state = sample();
        for (w, h, icons) in [
            (80, 24, false),
            (120, 36, true),
            (200, 50, true),
            (80, 24, true),
        ] {
            let buf = render(w, h, &state, icons);
            let body = shell(buf.area, &state).body;
            let list: Vec<String> = (0..h)
                .map(|y| span_text(&buf, y, body.x..body.right()))
                .collect();
            let find = |needle: &str| list.iter().position(|l| l.contains(needle));
            // The selected row: marker, bold slug, chip, overdue days,
            // price; the JOB.md status entry on the next line.
            let y = find("tk-denoise").expect("selected row");
            let row7 = &list[y];
            assert!(row7.starts_with("\u{258e} "), "{w}x{h}: {row7:?}");
            for part in ["delivered", "59d", "0.01"] {
                assert!(row7.contains(part), "{w}x{h}: {part:?} in {row7:?}");
            }
            assert_eq!(
                row7.contains("collect payment"),
                w >= 160,
                "{w}: next column"
            );
            let below = &list[y + 1];
            assert!(below.contains("09-20  预览已发送"), "{w}x{h}: {below:?}");
            assert!(!list.iter().any(|l| l.contains("(no status entry")));
            // Groups in the spec order, each under its heading.
            let owed = find("  owed  3").expect("owed heading");
            let paid = find("  paid  2").expect("paid heading");
            let prog = find("  in progress  1").expect("in progress heading");
            let queued = find("  queued  2").expect("queued heading");
            assert!(
                owed < y && y < paid && paid < prog && prog < queued,
                "{w}x{h}"
            );
            assert!(
                list[owed - 1].trim().is_empty(),
                "a blank row above a group"
            );
            // Archived orders are hidden until `a`.
            assert!(find(" o8 ").is_none());
            // Styles: marker accent, band behind the row, bold slug,
            // unpaid chip, overdue days bold unpaid.
            let y = y as u16;
            let at = |needle: &str| {
                let l = &list[usize::from(y)];
                body.x + crate::text::width(&l[..l.find(needle).unwrap()]) as u16
            };
            assert_eq!(buf[(body.x, y)].fg, Theme::DARK.accent);
            let slug = &buf[(at("tk-denoise"), y)];
            assert_eq!(slug.bg, Theme::DARK.sel);
            assert!(slug.modifier.contains(Modifier::BOLD));
            assert!(!slug.modifier.contains(Modifier::REVERSED));
            assert_eq!(
                buf[(body.right() - 1, y)].bg,
                Theme::DARK.sel,
                "band to the list edge"
            );
            assert_eq!(buf[(at("delivered"), y)].fg, Theme::DARK.unpaid);
            let days = &buf[(at("59d"), y)];
            assert_eq!(days.fg, Theme::DARK.unpaid);
            assert!(days.modifier.contains(Modifier::BOLD));
            // Unselected rows are not bold.
            let (y6, l6) = list
                .iter()
                .enumerate()
                .find(|(_, l)| l.contains(" o6 "))
                .unwrap();
            let x6 = body.x + crate::text::width(&l6[..l6.find("o6").unwrap()]) as u16;
            assert!(!buf[(x6, y6 as u16)].modifier.contains(Modifier::BOLD));
        }
    }

    #[test]
    fn no_icons_moves_the_slug_two_cells_left() {
        let state = sample();
        let col = |icons| {
            let buf = render(80, 24, &state, icons);
            let text = all(&buf);
            let l = text
                .lines()
                .find(|l| l.contains(" o6 "))
                .unwrap()
                .to_string();
            crate::text::width(&l[..l.find("o6").unwrap()])
        };
        assert_eq!(col(true), col(false) + 2);
        assert_eq!(col(true), 5);
    }

    #[test]
    fn chinese_titles_are_cut_by_width() {
        let state = sample();
        for (w, h, title) in [(80, 24, 21), (120, 36, 19), (200, 50, 40)] {
            let buf = render(w, h, &state, true);
            let body = shell(buf.area, &state).body;
            let (y, list_part) = (0..h)
                .map(|y| (y, span_text(&buf, y, body.x..body.right())))
                .find(|(_, l)| l.contains("tk-denoise"))
                .unwrap();
            let start = list_part.find("图像去噪").expect("title shown");
            let shown = &list_part[start..];
            assert!(
                shown.contains('\u{2026}'),
                "cut with an ellipsis: {list_part}"
            );
            assert!(
                crate::text::width(shown.trim_end()) <= title,
                "{w}: {shown:?} in {title} cells"
            );
            assert!(crate::text::width(shown.trim_end()) >= title - 1);
            // Nothing of the list row spills into the gutter.
            if let Some(pane) = shell(buf.area, &state).pane {
                for x in body.right()..pane.x {
                    assert_eq!(buf[(x, y)].symbol(), " ", "gutter at {x}");
                }
            }
        }
        // The full title is in the detail pane header.
        let text = all(&render(200, 50, &state, true));
        assert!(text.contains("图像去噪与超分辨率批处理工具开发及交付, 含批量脚本"));
    }

    /// `sample()` with a sent package, a client question and a long
    /// requirement change on the selected order.
    fn rich() -> UiState {
        let mut state = sample();
        for r in &mut state.data.orders {
            if r.order.id == 7 {
                r.order.platform = Some("xianyu".into());
                r.order.price_minor = Some(80000);
                r.job = crate::data::JobMd::parse(
                    "## Status\n\n- 2026-09-20: 预览已发送, 等客户确认.\n\n## Client questions\n\n- 2026-09-21: 输出 PNG 还是 TIFF?\n",
                );
                r.requirement_changes = vec![gig_core::models::RequirementChange {
                    id: 1,
                    order_id: 7,
                    description: "JC要求把模型优化的验证方案改为客户现有代码重跑方案, 每6条文件排序光谱划为一个伪小鼠, 按伪小鼠做5折分层交叉验证".into(),
                    price_delta_minor: 0,
                    created_at: "2026-08-21T10:00:00Z".into(),
                }];
                r.packages = vec![gig_core::models::Package {
                    id: 1,
                    order_id: 7,
                    package_id: "sers-colitis-analysis-delivery-2026-08-26".into(),
                    kind: gig_core::models::PackageKind::Full,
                    dir: String::new(),
                    manifest_path: String::new(),
                    zip_path: String::new(),
                    zip_sha256: None,
                    file_count: Some(3),
                    status: gig_core::models::PackageStatus::Sent,
                    checked_at: None,
                    sent_at: Some("2026-08-26T10:00:00Z".into()),
                    channel: Some(gig_core::models::Channel::Oss),
                    uploader: None,
                    remote_url: None,
                    short_url: Some("https://go.jczhang.cc/a30bd870".into()),
                    expires_at: None,
                    created_at: "2026-08-26T10:00:00Z".into(),
                    updated_at: "2026-08-26T10:00:00Z".into(),
                }];
            }
        }
        state
    }

    #[test]
    fn detail_sections_are_drawn() {
        let state = rich();
        let buf = render(200, 50, &state, true);
        let pane = shell(buf.area, &state).pane.unwrap();
        let rows: Vec<String> = (0..50)
            .map(|y| {
                span_text(&buf, y, pane.x..pane.right())
                    .trim_end()
                    .to_string()
            })
            .collect();
        let text = rows.join("\n");
        for s in [
            "tk-denoise \u{b7} \u{f0ad} tool \u{b7} xianyu",
            "delivered \u{b7} 59 days in status",
            "price          800 CNY",
            "cut             60%",
            "take-home      480 CNY",
            "Next",
            "Packages  1",
            "  sers-colitis-analysis-delivery-2026-08-26",
            "  full \u{b7} sent \u{b7} oss \u{b7} 08-26  go.jczhang.cc/a30bd870",
            "Latest status  1",
            "  09-20  预览已发送, 等客户确认.",
            "Client questions  1",
            "  ? 09-21  输出 PNG 还是 TIFF?",
            "Requirement changes  1",
            "Notes  1",
            "  09-21  client paid half",
            "Scorecard",
            "  score 4/5  \u{b7}  3 decisions  \u{b7}  0 repeat questions  \u{b7}  1 cleanup",
        ] {
            assert!(text.contains(s), "{s:?}\n{text}");
        }
        let next = rows.iter().find(|l| l.contains("collect payment")).unwrap();
        assert!(next.ends_with("p paid"), "{next:?}");
        // Headings bold, link underlined.
        let (hy, _) = rows
            .iter()
            .enumerate()
            .find(|(_, l)| l.starts_with("Packages"))
            .unwrap();
        assert!(buf[(pane.x, hy as u16)].modifier.contains(Modifier::BOLD));
        let (ly, l) = rows
            .iter()
            .enumerate()
            .find(|(_, l)| l.contains("go.jczhang"))
            .unwrap();
        let lx = pane.x + crate::text::width(&l[..l.find("go.jczhang").unwrap()]) as u16;
        assert!(buf[(lx, ly as u16)].modifier.contains(Modifier::UNDERLINED));
        // Overdue days are bold unpaid.
        let (sy, l) = rows
            .iter()
            .enumerate()
            .find(|(_, l)| l.contains("59 days"))
            .unwrap();
        let sx = pane.x + crate::text::width(&l[..l.find("59 days").unwrap()]) as u16;
        assert_eq!(buf[(sx, sy as u16)].fg, Theme::DARK.unpaid);
        // The empty sections are named once, together.
        let state = sample();
        let text = all(&render(200, 50, &state, true));
        assert!(
            text.contains("no packages \u{b7} no client questions \u{b7} no requirement changes"),
            "{text}"
        );
        assert!(!text.contains("(none)"));
        // Narrow: Enter shows the same detail full screen.
        let narrow = UiState {
            detail_open: true,
            ..rich()
        };
        let text = all(&render(80, 24, &narrow, false));
        assert!(text.contains("tk-denoise") && text.contains("Next"));
    }

    #[test]
    fn pane_at_120x36_keeps_links_whole_and_titles_long() {
        let state = rich();
        let buf = render(120, 36, &state, true);
        let s = shell(buf.area, &state);
        let pane = s.pane.unwrap();
        assert_eq!((s.body.width, pane.width), (76, 40));
        let rows: Vec<String> = (pane.y..pane.bottom())
            .map(|y| span_text(&buf, y, pane.x..pane.right()).trim().to_string())
            .collect();
        for r in &rows {
            assert!(crate::text::width(r) != 1, "a one-character row: {rows:?}");
        }
        assert!(
            rows.iter().any(|r| r == "go.jczhang.cc/a30bd870"),
            "{rows:?}"
        );
        // Pane entries are capped at 2 rows, the second ending in an ellipsis.
        let at = rows.iter().position(|r| r.starts_with("08-21")).unwrap();
        assert!(rows[at + 1].ends_with('\u{2026}'), "{rows:?}");
        // Every unselected order row shows at least 18 cells of its title.
        for r in state.order_list() {
            if r.order.id == 7 {
                continue;
            }
            let slug = format!(" {} ", r.order.slug);
            let l = (0..36)
                .map(|y| span_text(&buf, y, s.body.x..s.body.right()))
                .find(|l| l.contains(&slug))
                .unwrap();
            let title = l[l.find("Order ").unwrap()..].trim_end();
            assert!(
                crate::text::width(title) >= 18 || title == r.order.title,
                "{l:?}"
            );
        }
    }

    #[test]
    fn full_screen_detail_scrolls_to_every_section() {
        let mut state = UiState {
            detail_open: true,
            ..rich()
        };
        // A long note list and a multi-line change, so it overflows 24 rows.
        for r in &mut state.data.orders {
            if r.order.id == 7 {
                r.notes = crate::data::parse_notes(
                    &(1..=5)
                        .map(|i| {
                            format!("[2026-09-2{i}T10:00:00Z] note number {i} with some words\n")
                        })
                        .collect::<String>(),
                );
            }
        }
        let press = |s: &mut UiState, code| {
            s.handle_key(
                crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
                80,
            )
        };
        let text = all(&render(80, 24, &state, false));
        assert!(text.contains("more  PgDn"), "{text}");
        assert!(!text.contains("Scorecard"), "{text}");
        let mut seen = String::new();
        for _ in 0..5 {
            press(&mut state, crossterm::event::KeyCode::PageDown);
            seen.push_str(&all(&render(80, 24, &state, false)));
        }
        for s in ["Notes", "note number 5", "Scorecard", "score 4", "PgUp"] {
            assert!(seen.contains(s), "{s}: {seen}");
        }
        // Moving the selection starts the next order at the top.
        press(&mut state, crossterm::event::KeyCode::Down);
        assert_eq!(state.detail_scroll.offset, 0);
        // Continuation rows keep the item indent.
        let narrow = all(&render(40, 40, &state, false));
        for l in narrow.lines() {
            assert!(!l.starts_with(" report"), "{l}");
        }
    }

    #[test]
    fn light_theme_band_marker_and_status_colours() {
        let mut state = sample();
        let buf = render_with(80, 24, &state, false, Theme::LIGHT);
        let text = all(&buf);
        let (y, row7) = line_of(&text, "tk-denoise").unwrap();
        let y = y as u16;
        assert!(row7.starts_with(" \u{258e}"), "selection marker: {row7}");
        assert_eq!(buf[(1, y)].fg, Theme::LIGHT.accent);
        let x = crate::text::width(&row7[..row7.find("tk-denoise").unwrap()]) as u16;
        assert_eq!(buf[(x, y)].bg, Theme::LIGHT.sel);
        let chip_x = crate::text::width(&row7[..row7.find("delivered").unwrap()]) as u16;
        assert_eq!(buf[(chip_x, y)].fg, Theme::LIGHT.unpaid);
        // The second line: on the band, marker too, date and entry muted.
        let below = text.lines().nth(usize::from(y) + 1).unwrap();
        assert!(below.starts_with(" \u{258e}"), "{below}");
        let sx = crate::text::width(&below[..below.find("预览").unwrap()]) as u16;
        assert_eq!(buf[(sx, y + 1)].fg, Theme::LIGHT.muted);
        assert_eq!(buf[(sx, y + 1)].bg, Theme::LIGHT.sel);
        // Paid in warranty is amber; paid with the warranty over is muted.
        let mut paid = 0;
        for r in &state.data.orders {
            if r.order.status == gig_core::models::OrderStatus::Paid {
                paid += 1;
                let (py, l) = line_of(&text, &format!(" {} ", r.order.slug)).unwrap();
                let px = crate::text::width(&l[..l.find("paid").unwrap()]) as u16;
                let want = if r.next_action == "archive" {
                    Theme::LIGHT.muted
                } else {
                    Theme::LIGHT.warranty
                };
                assert_eq!(buf[(px, py as u16)].fg, want, "{}", r.order.slug);
            }
        }
        assert_eq!(paid, 2, "one in warranty, one to archive");
        state.view = View::History;
        let buf = render_with(120, 36, &state, true, Theme::LIGHT);
        let text = all(&buf);
        assert!(
            text.contains("2026-10-10") && text.contains("4/5"),
            "{text}"
        );
        // Archived rows entirely in `archived`.
        let (ay, l) = line_of(&text, " o8 ").unwrap();
        let ax = crate::text::width(&l[..l.find("o8").unwrap()]) as u16;
        assert_eq!(buf[(ax, ay as u16)].fg, Theme::LIGHT.archived);
        // Empty cells are dim dots, never `-`.
        let (_, l2) = line_of(&text, " o2 ").unwrap();
        assert!(l2.contains('\u{b7}') && !l2.contains(" - "), "{l2}");
    }

    #[test]
    fn banner_keeps_owed_when_narrow() {
        let mut state = UiState::default();
        state.data.money.outstanding.gross = 123_456_700;
        state.data.money.month.gross = 234_567_800;
        state.data.money.year.gross = 987_654_321;
        state.data.today = "2026-09-29".into();
        let top = row(&render(80, 24, &state, true), 0);
        assert!(top.contains("  owed 1,234,567  "), "{top}");
        assert!(!top.contains("2026"), "{top}");
        // Mockup 17.2: tabs 2 apart, CNY and the year dropped.
        let mut state = UiState::default();
        state.data.today = "2026-09-29".into();
        state.data.money.outstanding.gross = 160_000;
        state.data.money.year.gross = 2_955_000;
        let top = row(&render(80, 24, &state, true), 0);
        assert_eq!(
            top,
            " gig  1 Orders  2 Drafts  3 Money  4 History               owed 1,600  \u{b7}  sep 0 "
        );
        // Very narrow: only the active tab keeps its word.
        state.data.money.outstanding.gross = 12_345_678_900;
        let top = row(&render(60, 16, &state, true), 0);
        assert!(top.starts_with(" gig  1 Orders  2  3  4"), "{top}");
    }

    #[test]
    fn footer_follows_the_selected_order() {
        let state = sample();
        // o7 (tk-denoise, delivered) is selected.
        let wide = row(&render(200, 50, &state, true), 49);
        assert!(
            wide.starts_with(
                " p paid  y copy link  n note  k score   \u{b7}   / filter  a archived  N new  , settings  ? keys  q quit"
            ),
            "{wide}"
        );
        // Mockup 17.2: Enter detail first, k score and the optional pairs go.
        let narrow = row(&render(80, 24, &state, false), 23);
        assert_eq!(
            narrow.trim_end(),
            " Enter detail  p paid  y copy link  n note   \u{b7}   / filter  ? keys  q quit"
        );
        // Keys bold, labels muted, the dot dim.
        let buf = render(80, 24, &state, false);
        assert!(buf[(1, 23)].modifier.contains(Modifier::BOLD));
        assert_eq!(buf[(7, 23)].fg, Theme::DARK.muted);
        // From History, the detail shows the order keys.
        let hist = UiState {
            view: View::History,
            detail_open: true,
            ..sample()
        };
        let line = row(&render(200, 50, &hist, true), 49);
        assert!(line.starts_with(" Esc back  p paid"), "{line}");
        // Queued: start, price, note, cancel.
        let mut q = sample();
        q.selected = Some(1);
        let line = row(&render(200, 50, &q, true), 49);
        assert!(
            line.starts_with(" s start  $ price  n note  x cancel"),
            "{line}"
        );
        // Money names its own keys.
        let mut m = sample();
        m.view = View::Money;
        let line = row(&render(120, 36, &m, true), 35);
        assert!(
            line.contains("1-4 views  T theme  , settings  ? keys  q quit"),
            "{line}"
        );
        // Long filter text keeps its end and the cursor.
        let mut f = UiState::default();
        f.filters[0].editing = true;
        f.filters[0].text = format!("{}END", "x".repeat(100));
        assert!(row(&render(80, 24, &f, false), 1).contains("END\u{258f}"));
        // A long refresh error is cut and the keys stay.
        let e = UiState {
            error: Some(format!("db: {}", "locked ".repeat(30))),
            ..UiState::default()
        };
        let buf = render(80, 24, &e, false);
        assert!(row(&buf, 1).contains('\u{2026}'));
        assert!(row(&buf, 23).contains("? keys"));
    }

    /// Money with the months of mockup 17.3: Apr to Jul paid, Sep (the
    /// current month) zero.
    fn money_state() -> UiState {
        let mut state = sample();
        state.view = View::Money;
        state.data.money = crate::data::money::Money::default();
        state.data.money.by_month = crate::data::money::last_months("2026-09-29", 12)
            .into_iter()
            .map(|label| {
                let gross = match label.as_str() {
                    "2026-04" => 1_000_000,
                    "2026-05" => 310_000,
                    "2026-06" => 1_285_000,
                    "2026-07" => 360_000,
                    _ => 0,
                };
                crate::data::money::Month {
                    label,
                    amount: crate::data::money::Amount {
                        gross,
                        take_home: gross * 6 / 10,
                    },
                }
            })
            .collect();
        state.data.money.outstanding.gross = 160_000;
        state.data.money.outstanding.take_home = 96_000;
        state.data.money.year.gross = 2_955_000;
        state.data.money.year.take_home = 1_773_000;
        for (id, days) in [(7, 34), (6, 0)] {
            state.data.money.owed.push(crate::data::money::Owed {
                order_id: id,
                slug: format!("o{id}"),
                price_minor: Some(80_000),
                currency: "CNY".into(),
                days: Some(days),
            });
        }
        state
    }

    #[test]
    fn money_matches_mockup_17_3() {
        let state = money_state();
        let buf = render(120, 36, &state, true);
        let rows: Vec<String> = (0..36)
            .map(|y| row(&buf, y).trim_end().to_string())
            .collect();
        assert_eq!(rows[2], " outstanding                            received in September                  received in 2026");
        assert_eq!(rows[3], " 1,600 CNY                              0 CNY                                  29,550 CNY");
        assert_eq!(rows[4], " take-home 960 \u{b7} 2 orders               take-home 0                            take-home 17,730 \u{b7} 4 months");
        assert_eq!(rows[6], " Received per month                                                                         Oct 2025 - Sep 2026  \u{b7}  CNY");
        assert_eq!(rows[8], format!("{}12.9k", " ".repeat(75)));
        assert_eq!(
            rows[10],
            format!(
                "{}10.0k{}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
                " ".repeat(57),
                " ".repeat(13)
            )
        );
        let baseline: String = (0..12)
            .map(|m| {
                let mid = if (6..10).contains(&m) {
                    "\u{2500}"
                } else {
                    "\u{2508}"
                };
                format!("\u{2500}\u{2500}{}\u{2500}\u{2500}", mid.repeat(5))
            })
            .collect();
        assert_eq!(rows[19], format!(" {baseline}"));
        assert_eq!(rows[20], "    Oct      Nov      Dec      Jan      Feb      Mar      Apr      May      Jun      Jul      Aug      Sep");
        assert_eq!(rows[21], "    2025                       2026");
        assert_eq!(rows[23], " Outstanding  2               1,600");
        assert_eq!(
            rows[24],
            "     order                      CNY  since  title"
        );
        assert!(
            rows[25].starts_with(" \u{258e} \u{f0ad} o7                         800    34d  "),
            "{:?}",
            rows[25]
        );
        // Colours: current month's zero footprint in bar_now, past bars in
        // bar, labels not in the bar colour, owed value bold unpaid.
        assert_eq!(buf[(104, 19)].fg, Theme::DARK.bar_now);
        assert_eq!(buf[(3, 19)].fg, Theme::DARK.dim);
        assert_eq!(buf[(1, 19)].fg, Theme::DARK.border);
        assert_eq!(buf[(76, 9)].fg, Theme::DARK.bar);
        assert_eq!(buf[(76, 8)].fg, Theme::DARK.text);
        assert_eq!(buf[(1, 3)].fg, Theme::DARK.unpaid);
        assert!(buf[(1, 3)].modifier.contains(Modifier::BOLD));
        let sep = &buf[(103, 20)];
        assert!(
            sep.modifier.contains(Modifier::BOLD),
            "current month label bold"
        );
    }

    #[test]
    fn money_chart_rows_and_months_at_every_size() {
        let state = money_state();
        for (w, h, plot) in [(80, 24, 6u16), (120, 36, 10), (200, 50, 10)] {
            let buf = render(w, h, &state, true);
            let rows: Vec<String> = (0..h).map(|y| row(&buf, y)).collect();
            let months = rows.iter().position(|r| r.contains("Nov")).unwrap();
            for m in [
                "Oct", "Nov", "Dec", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep",
            ] {
                assert!(rows[months].contains(m), "{w}x{h}: {m}");
            }
            // Chart: label row, plot rows, baseline, months, years.
            let heading = rows
                .iter()
                .position(|r| r.contains("Received per month"))
                .unwrap();
            let first = heading + 2;
            let baseline = months - 1;
            assert_eq!(baseline - first, usize::from(plot) + 1, "{w}x{h}");
            assert_eq!(months + 1 - first + 1, usize::from(plot) + 4);
            assert!(rows[baseline].contains('\u{2508}') && rows[baseline].contains('\u{2500}'));
            assert!(rows[months + 1].contains("2025") && rows[months + 1].contains("2026"));
            assert!(rows.iter().any(|r| r.contains("12.9k")));
        }
        // No payments: a dotted baseline and a line saying so.
        let mut empty = money_state();
        for m in &mut empty.data.money.by_month {
            m.amount = crate::data::money::Amount::default();
        }
        let buf = render(120, 36, &empty, true);
        let text = all(&buf);
        assert!(text.contains("no payments in the last 12 months"));
        assert!(!text.contains('\u{2500}'), "{text}");
        // The current month's footprint is `bar_now`, the rest `dim`.
        let (y, base) = line_of(&text, "\u{2508}").unwrap();
        let fgs: Vec<_> = (0..buf.area.width)
            .filter(|&x| buf[(x, y as u16)].symbol() == "\u{2508}")
            .map(|x| buf[(x, y as u16)].fg)
            .collect();
        assert!(fgs.contains(&Theme::DARK.dim), "{base}");
        let now = fgs.iter().filter(|&&c| c == Theme::DARK.bar_now).count();
        assert!(now > 0 && now < 10, "{base}");
        let first_now = fgs.iter().position(|&c| c == Theme::DARK.bar_now).unwrap();
        assert!(first_now > fgs.len() * 10 / 12, "{base}");
    }

    #[test]
    fn money_table_selects_and_says_what_it_leaves_out() {
        let mut state = money_state();
        for i in 0..20 {
            state.data.money.owed.push(crate::data::money::Owed {
                order_id: 100 + i,
                slug: format!("owed-{i:02}"),
                price_minor: Some(100_000 + i),
                currency: "CNY".into(),
                days: Some(30 - i),
            });
        }
        let text = all(&render(80, 24, &state, false));
        assert!(text.contains("\u{2193} 21 more"), "{text}");
        let text = all(&render(120, 36, &state, false));
        assert!(text.contains("owed-00") && text.contains(" more"), "{text}");
        assert!(!text.contains("owed-19"));
        // The selection moves in the table and stays in view.
        for _ in 0..21 {
            state.handle_key(
                crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Down,
                    crossterm::event::KeyModifiers::NONE,
                ),
                120,
            );
        }
        assert_eq!(state.selected_owed(), Some(119));
        let text = all(&render(120, 36, &state, false));
        let (_, l) = line_of(&text, "owed-19").expect("selected row in view");
        assert!(l.starts_with(" \u{258e}"), "{l}");
        // `/` does nothing in Money.
        let slash = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('/'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.handle_key(slash, 80);
        assert!(!state.filter().editing);
    }

    fn new_order_popup(error: bool) -> popup::Popup {
        use crate::popup::{Field, Form};
        let mut slug = Field::text("slug", "sers-colitis-v2");
        if error {
            slug.error = Some("slug already exists".into());
        }
        popup::Popup::Form(Form::new(
            "new order",
            crate::actions::FormKind::NewOrder,
            vec![
                slug,
                Field::text("title", "小鼠结肠炎 SERS 二期"),
                Field::text("price", ""),
                Field::select("type", &["tool", "cv_ml"], "cv_ml"),
                Field::toggle("from draft", false),
                Field::editor("client words", ""),
            ],
        ))
    }

    #[test]
    fn popups_scrim_the_screen_and_fix_cut_glyphs() {
        for (w, h) in [(80, 24), (120, 36), (200, 50)] {
            let state = UiState {
                popup: Some(new_order_popup(false)),
                ..sample()
            };
            let buf = render(w, h, &state, true);
            // The box: 64 wide (or W - 4), top on the upper third.
            let top = (0..h).find(|&y| row(&buf, y).contains("\u{256d}")).unwrap();
            let r = row(&buf, top);
            let left = crate::text::width(&r[..r.find('\u{256d}').unwrap()]) as u16;
            let width = 64.min(w - 4);
            assert_eq!(left, (w - width) / 2, "{w}x{h}");
            let height = (top..h)
                .find(|&y| row(&buf, y).contains('\u{256f}'))
                .unwrap()
                + 1
                - top;
            assert_eq!(top, ((h - height) / 3).max(1), "{w}x{h}");
            let rect = Rect::new(left, top, width, height);
            for y in 0..h {
                for x in 0..w {
                    let c = &buf[(x, y)];
                    // The backend never styles the trailing half of a
                    // wide glyph.
                    let trailing = x > 0 && crate::text::width(buf[(x - 1, y)].symbol()) > 1;
                    if rect.contains((x, y).into()) || trailing {
                        continue;
                    }
                    assert_eq!(c.fg, Theme::DARK.dim, "{w}x{h} scrim at {x},{y}");
                    assert!(!c.modifier.contains(Modifier::BOLD | Modifier::UNDERLINED));
                }
            }
            // No half glyph on either side of the box.
            for y in rect.top()..rect.bottom() {
                assert_ne!(buf[(rect.right(), y)].symbol(), "", "{w}x{h} at row {y}");
                let l = buf[(rect.left() - 1, y)].symbol();
                assert!(
                    crate::text::width(l) <= 1,
                    "{w}x{h} wide glyph cut at row {y}"
                );
            }
            // The fill is `surface`.
            assert_eq!(buf[(left + 3, top + 1)].bg, Theme::DARK.surface);
        }
    }

    #[test]
    fn form_anatomy() {
        let state = UiState {
            popup: Some(new_order_popup(true)),
            ..UiState::default()
        };
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let icons = Icons::new(true);
        let theme = Theme::DARK;
        term.draw(|f| {
            draw(
                f,
                &RenderCx {
                    state: &state,
                    theme: &theme,
                    icons: &icons,
                },
            )
        })
        .unwrap();
        let buf = term.backend().buffer().clone();
        let text = all(&buf);
        let (y, slug) = line_of(&text, "sers-colitis-v2").unwrap();
        let y = y as u16;
        // Marker in the padding, label right-aligned in 14 cells, the band.
        assert!(
            slug.contains("\u{2502}\u{258e}           slug  sers-colitis-v2"),
            "{slug}"
        );
        let mx = crate::text::width(&slug[..slug.find('\u{258e}').unwrap()]) as u16;
        assert_eq!(buf[(mx, y)].fg, Theme::DARK.accent);
        assert_eq!(buf[(mx + 30, y)].bg, Theme::DARK.sel);
        // The validation line under the value.
        let below = text.lines().nth(usize::from(y) + 1).unwrap();
        assert!(
            below.contains("                  ! slug already exists"),
            "{below}"
        );
        // Placeholder, select arrows, toggle, editor hint, footer.
        for s in [
            "price  e.g. 800",
            "type  \u{2039} cv_ml \u{203a}",
            "from draft  [ ] no",
            "client words  Enter opens $EDITOR",
            "Tab next  Space choose  Enter submit  Esc cancel",
        ] {
            assert!(text.contains(s), "{s}\n{text}");
        }
        let (py, p) = line_of(&text, "e.g. 800").unwrap();
        let px = crate::text::width(&p[..p.find("e.g.").unwrap()]) as u16;
        assert!(buf[(px, py as u16)].modifier.contains(Modifier::ITALIC));
        // The terminal cursor sits after the typed slug.
        let cur = term.get_cursor_position().unwrap();
        let sx = crate::text::width(&slug[..slug.find("sers").unwrap()]) as u16;
        assert_eq!((cur.x, cur.y), (sx + 15, y));
    }

    /// A terminal grid that, like tmux or xterm, clears the other half of a
    /// wide glyph to the default cell when either half is overwritten.
    struct Screen(Vec<Vec<(ratatui::buffer::Cell, u16)>>);

    impl Screen {
        fn new(w: u16, h: u16) -> Self {
            Screen(vec![
                vec![(Default::default(), 1); usize::from(w)];
                usize::from(h)
            ])
        }

        /// Width 0 marks the covered half of a wide glyph.
        fn put(&mut self, x: u16, y: u16, c: &ratatui::buffer::Cell) {
            let row = &mut self.0[usize::from(y)];
            let (x, w) = (usize::from(x), usize::from(c.cell_width().max(1)));
            for k in x..(x + w).min(row.len()) {
                let (start, len) = match row[k].1 {
                    0 => (k - 1, 2),
                    n => (k, usize::from(n)),
                };
                if len > 1 {
                    for j in start..(start + len).min(row.len()) {
                        row[j] = (Default::default(), 1);
                    }
                }
            }
            row[x] = (c.clone(), w as u16);
            for k in x + 1..(x + w).min(row.len()) {
                row[k] = (Default::default(), 0);
            }
        }
    }

    #[test]
    fn moving_cjk_leaves_no_hole_in_the_page() {
        let (w, h) = (120, 36);
        let icons = Icons::new(true);
        let theme = Theme::DARK;
        let frame = |title: &str| {
            let mut state = sample();
            for r in &mut state.data.orders {
                if r.order.id == 7 {
                    r.order.title = title.into();
                }
            }
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            let done = term
                .draw(|f| {
                    draw(
                        f,
                        &RenderCx {
                            state: &state,
                            theme: &theme,
                            icons: &icons,
                        },
                    )
                })
                .unwrap();
            done.buffer.clone()
        };
        // The pane title shifts one cell left: `与` covered one more cell
        // than `具` now does.
        let titles = [
            "小鼠结肠炎 SERS 光谱分析与图表改版",
            "TK DTF 生产图纵向压缩工具",
            "x小鼠结肠炎 SERS 光谱分析与图表改版",
            "小鼠结肠炎 SERS 光谱分析与图表改版",
        ];
        let mut prev = Buffer::empty(Rect::new(0, 0, w, h));
        let mut screen = Screen::new(w, h);
        for title in titles {
            let next = frame(title);
            for (x, y, c) in prev.diff(&next) {
                screen.put(x, y, c);
            }
            for (y, row) in screen.0.iter().enumerate() {
                for (x, (c, cw)) in row.iter().enumerate() {
                    assert!(
                        *cw == 0 || c.bg != ratatui::style::Color::Reset,
                        "{title}: hole at {x},{y}"
                    );
                }
            }
            prev = next;
        }
    }

    #[test]
    fn every_view_every_theme_every_size() {
        let base = rich();
        let mut states = Vec::new();
        for v in View::ALL {
            states.push(UiState {
                view: v,
                ..base.clone()
            });
        }
        states.push(UiState {
            detail_open: true,
            ..base.clone()
        });
        states.push(UiState {
            help_open: true,
            ..base.clone()
        });
        states.push(UiState {
            popup: Some(new_order_popup(true)),
            ..base.clone()
        });
        states.push(UiState {
            view: View::Money,
            ..money_state()
        });
        states.push(UiState::default());
        let mut themes: Vec<Theme> = Theme::BUILTIN.to_vec();
        themes.push(Theme::DARK.for_mode(crate::theme::ColorMode::NoColor));
        themes.push(Theme::DARK.for_mode(crate::theme::ColorMode::Indexed));
        for theme in &themes {
            for (w, h) in [
                (80, 24),
                (120, 36),
                (200, 50),
                (60, 16),
                (110, 30),
                (160, 40),
            ] {
                for icons in [true, false] {
                    for st in &states {
                        let buf = render_with(w, h, st, icons, theme.clone());
                        assert_eq!(buf.area, Rect::new(0, 0, w, h));
                        // Every row fits the frame exactly.
                        for y in 0..h {
                            assert!(crate::text::width(&row(&buf, y)) <= usize::from(w));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn toggle_and_filter_change_the_rows() {
        let mut state = sample();
        state.show_closed = true;
        let text = all(&render(80, 24, &state, false));
        assert!(line_of(&text, " o8 ").is_some(), "archived shown with a");
        state.filters[0].text = "denoise".into();
        let text = all(&render(80, 24, &state, false));
        assert!(line_of(&text, "tk-denoise").is_some());
        assert!(line_of(&text, " o6 ").is_none());
        state.filters[0].text = "nothing-matches".into();
        let text = all(&render(80, 24, &state, false));
        assert!(text.contains("no order matches the filter"));
    }

    #[test]
    fn selection_stays_visible_in_a_short_terminal() {
        let mut state = sample();
        state.selected = Some(2); // last row (queued, newest)
        let buf = render(80, 16, &state, false);
        let text = all(&buf);
        assert!(line_of(&text, "o2 ").is_some(), "{text}");
        // Sticky heading: the first row under the header names the group
        // of the first visible order.
        assert_eq!(row(&buf, 2).split_whitespace().next(), Some("order"));
        for h in 16..30 {
            let buf = render(80, h, &state, false);
            let first = (3..h)
                .map(|y| row(&buf, y))
                .find(|l| !l.trim().is_empty())
                .unwrap();
            let heading = ["owed  ", "paid  ", "in progress  ", "queued  "]
                .iter()
                .any(|w| first.starts_with(&format!("   {w}")));
            assert!(heading, "{h}: {first:?}");
            assert!(all(&buf).contains(" o2 "), "{h}");
        }
    }

    #[test]
    fn history_money_and_drafts_render() {
        let mut state = sample();
        state.view = View::History;
        let text = all(&render(200, 50, &state, true));
        assert!(text.contains("4/5"), "scorecard score");
        assert!(line_of(&text, " o8 ").is_some(), "archived in history");

        state.view = View::Drafts;
        state.data.drafts.push(gig_core::models::Draft {
            id: 1,
            slug: "dr-ocr".into(),
            title: Some("发票识别".into()),
            material_path: Some("/mnt/m/ocr".into()),
            project_type: None,
            notes_dir: "/nonexistent".into(),
            status: gig_core::models::DraftStatus::Open,
            drop_reason: None,
            notes_snapshot: None,
            promoted_order_id: None,
            created_at: "2026-09-19T00:00:00Z".into(),
            closed_at: None,
        });
        let text = all(&render(80, 24, &state, false));
        let (_, l) = line_of(&text, "dr-ocr").unwrap();
        assert!(l.contains("10d") && l.contains("发票识别") && l.contains("/mnt/m/ocr"));
    }

    fn settings_state() -> UiState {
        use gig_core::config::Config;
        let file = Config::default();
        let mut running = file.clone();
        running.tui.icons = false;
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/x".into());
        let mut s = crate::settings::Settings::new(
            Ok(file),
            &running,
            &std::path::Path::new(&home).join(".config/gig/config.toml"),
        );
        s.cursor = 2;
        s.errors[2] =
            Some("invalid input: tui.refresh_seconds must be a whole number from 0 to 60".into());
        UiState {
            settings: Some(s),
            ..sample()
        }
    }

    #[test]
    fn settings_overlay_at_80x24_and_160x45() {
        let state = settings_state();
        for (w, h) in [(80u16, 24u16), (160, 45)] {
            let buf = render(w, h, &state, true);
            let text = all(&buf);
            if std::env::var_os("GIG_PRINT").is_some() {
                println!("{text}");
            }
            // Every schema row: key path, value and help.
            for e in gig_core::config::schema::ENTRIES {
                assert!(text.contains(e.key), "{w}x{h}: {}\n{text}", e.key);
                assert!(text.contains(e.help), "{w}x{h}: {}\n{text}", e.help);
            }
            for s in [
                "Settings",
                "writes ~/.config/gig/config.toml",
                "Dashboard",
                "General",
                "\u{2039} gig-dark \u{203a}",
                "[x] yes",
                "this session no",
                "! invalid input: tui.refresh_seconds must be a whole number",
                "0.6",
                "CNY",
            ] {
                assert!(text.contains(s), "{w}x{h}: {s}\n{text}");
            }
            // The orders list is hidden; the footer names the row's keys.
            assert!(!text.contains("tk-denoise"), "{text}");
            let footer = row(&buf, h - 1);
            assert!(footer.contains("+ - step  Enter type"), "{footer}");
            assert!(footer.contains("Esc close"), "{footer}");
            for y in 0..h {
                assert!(crate::text::width(&row(&buf, y)) <= usize::from(w));
            }
            // The cursor row: accent marker on the sel band.
            let (y, _) = line_of(&text, "tui.refresh_seconds").unwrap();
            let y = y as u16;
            assert_eq!(buf[(1, y)].symbol(), crate::views::SELECTED_MARK);
            assert_eq!(buf[(1, y)].fg, Theme::DARK.accent);
            assert_eq!(buf[(40, y)].bg, Theme::DARK.sel);
            assert_eq!(buf[(40, y + 1)].bg, Theme::DARK.sel, "help line too");
        }
        // Typing: the value shows what is typed and the cursor sits after it.
        let mut state = settings_state();
        state.settings.as_mut().unwrap().edit = Some("61".into());
        let text = all(&render(80, 24, &state, true));
        let (_, l) = line_of(&text, "tui.refresh_seconds").unwrap();
        assert!(l.trim_end().ends_with("61"), "{l}");
        assert!(row(&render(80, 24, &state, true), 23).contains("Enter save"));
    }

    #[test]
    fn settings_scroll_keeps_the_cursor_row_in_view() {
        let mut state = settings_state();
        state.settings.as_mut().unwrap().cursor = 6;
        let text = all(&render(60, 16, &state, false));
        assert!(text.contains("general.default_cut_ratio"), "{text}");
        assert!(text.contains("above"), "{text}");
        state.settings.as_mut().unwrap().cursor = 0;
        let text = all(&render(60, 16, &state, false));
        assert!(text.contains("tui.theme"), "{text}");
        assert!(text.contains("more"), "{text}");
    }

    fn picker_state(current: &str) -> UiState {
        let mut catalog = crate::themes::Catalog::load(std::path::Path::new("/nonexistent"));
        let mut mine = Theme::NORD.clone();
        mine.name = "mocha-soft".into();
        catalog.themes.push(mine);
        catalog.user.insert("mocha-soft".into());
        catalog.broken.push(crate::themes::Broken {
            path: "/t/murky.toml".into(),
            name: "murky".into(),
            detail: "missing key \"bar\"".into(),
        });
        UiState {
            picker: Some(crate::picker::Picker::new(
                &catalog,
                current,
                crate::theme::ColorMode::TrueColor,
            )),
            ..sample()
        }
    }

    #[test]
    fn theme_picker_lists_marks_and_previews() {
        let mut state = picker_state("gig-dark");
        for (w, h) in [(80u16, 24u16), (160, 45)] {
            let buf = render(w, h, &state, true);
            let text = all(&buf);
            if std::env::var_os("GIG_PRINT").is_some() {
                println!("{text}");
            }
            let names: Vec<usize> = Theme::BUILTIN
                .iter()
                .map(|t| line_of(&text, &format!(" {} ", t.name)).unwrap().0)
                .collect();
            assert!(names.windows(2).all(|p| p[0] < p[1]), "built-in order");
            let (user, l) = line_of(&text, "mocha-soft").unwrap();
            assert!(user > names[7] && l.contains("file"), "{l}");
            let (broken, l) = line_of(&text, "murky").unwrap();
            assert!(broken > user && l.contains("! missing key \"bar\""), "{l}");
            let (y, l) = line_of(&text, "gig-dark ").unwrap();
            assert!(l.contains("current"), "{l}");
            // Five swatches: bg, text, accent, unpaid, warranty.
            let x = l.find('\u{2588}').map(|b| l[..b].chars().count()).unwrap() as u16;
            let t = Theme::DARK;
            for (k, c) in [t.bg, t.text, t.accent, t.unpaid, t.warranty]
                .iter()
                .enumerate()
            {
                assert_eq!(buf[(x + 3 * k as u16, y as u16)].fg, *c, "swatch {k}");
            }
            assert!(text.contains("Enter keep  c copy  Esc restore"), "{text}");
        }
        // Moving previews the whole screen: the page bg is nord's.
        state.picker.as_mut().unwrap().cursor = 6;
        let buf = render(80, 24, &state, true);
        assert_eq!(buf[(0, 0)].bg, Theme::NORD.bg);
        let text = all(&buf);
        let (_, l) = line_of(&text, "gig-dark ").unwrap();
        assert!(l.contains("current"), "the current mark stays: {l}");
        // Closing restores (the app's theme was never changed).
        state.picker = None;
        assert_eq!(render(80, 24, &state, true)[(0, 0)].bg, Theme::DARK.bg);
    }
}
