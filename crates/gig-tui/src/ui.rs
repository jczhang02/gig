//! The frame (TUI-DESIGN.md sections 6 and 7): banner on row 0, the
//! message row, the view body, and the key hints on the last row.
//! Whitespace and colour instead of borders.

use crate::app::{Tone, UiState, View};
use crate::icons::Icons;
use crate::theme::Theme;
use crate::{help, popup, text, views};
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
        && match state.view {
            View::Orders => true,
            View::Drafts => state.notes_pane.is_some(),
            _ => false,
        };
    let (body, pane) = if state.detail_open {
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
    if cx.state.detail_open {
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
    if let Some(p) = &cx.state.popup {
        popup::render(frame, area, p, cx.theme);
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
        assert!(text.contains("no active orders"), "{text}");
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
        assert!(all(&buf).contains("no order selected"));
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
            (View::Drafts, "material"),
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
                "next theme",
                "mark sent",
                "when a package is checked",
                "theme gig-dark",
                "\u{256d}",
            ] {
                assert!(text.contains(s), "{w}x{h}: {s}\n{text}");
            }
        }
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
    fn orders_rows_at_80x24_and_200x50() {
        let state = sample();
        for (w, h, icons) in [(80, 24, false), (200, 50, true), (80, 24, true)] {
            let buf = render(w, h, &state, icons);
            let text = all(&buf);
            // The selected row: slug, status chip, next action, days, price
            // on one line, and the JOB.md status on the next.
            let (y, row7) = line_of(&text, "tk-denoise").expect("selected row");
            for part in ["delivered", "collect", "59d", "1"] {
                assert!(row7.contains(part), "{w}x{h}: {part:?} in {row7:?}");
            }
            let below = text.lines().nth(y + 1).unwrap();
            assert!(below.contains("预览已发送"), "{w}x{h}: {below:?}");
            // Sorted: unpaid first, queued last.
            let (y6, _) = line_of(&text, " o6 ").unwrap();
            let (y1, _) = line_of(&text, " o1 ").unwrap();
            assert!(y < y6 && y6 < y1, "{w}x{h} order");
            // Archived orders are hidden until `a`.
            assert!(line_of(&text, " o8 ").is_none());
            // The selection band covers the row, not reverse video.
            let slug_x = row7.find("tk-denoise").unwrap();
            let x = crate::text::width(&row7[..slug_x]) as u16;
            let c = &buf[(x, y as u16)];
            assert_eq!(c.bg, Theme::DARK.sel);
            assert!(!c.modifier.contains(Modifier::REVERSED));
            // Status colour: unpaid red.
            let chip_x = crate::text::width(&row7[..row7.find("delivered").unwrap()]) as u16;
            assert_eq!(buf[(chip_x, y as u16)].fg, Theme::DARK.unpaid);
        }
    }

    #[test]
    fn chinese_titles_are_cut_by_width() {
        let state = sample();
        // At 200 columns the list is 55% wide and the detail is beside it.
        let buf = render(200, 50, &state, true);
        let body = shell(buf.area, &state).body;
        let text = all(&buf);
        let (y, _) = line_of(&text, "tk-denoise").unwrap();
        let list_part = span_text(&buf, y as u16, body.x..body.right());
        assert!(list_part.contains("图像去噪"), "{list_part}");
        assert!(
            list_part.contains('\u{2026}'),
            "cut with an ellipsis: {list_part}"
        );
        // Nothing of the list row spills into the gutter.
        for x in body.right()..body.right() + 2 {
            assert_eq!(buf[(x, y as u16)].symbol(), " ", "gutter at {x}");
        }
        // The full title is in the detail pane header.
        assert!(text.contains("图像去噪与超分辨率批处理工具开发及交付, 含批量脚本"));
    }

    #[test]
    fn detail_sections_are_drawn() {
        let state = sample();
        let text = all(&render(200, 50, &state, true));
        for s in [
            "Next action",
            "collect payment",
            "Packages",
            "Latest status",
            "预览已发送",
            "Client questions",
            "Requirement changes",
            "Notes",
            "client paid half",
            "Scorecard",
            "score 4",
            "cut 60%",
        ] {
            assert!(text.contains(s), "{s}");
        }
        // Narrow: Enter shows the same detail full screen.
        let narrow = UiState {
            detail_open: true,
            ..sample()
        };
        let text = all(&render(80, 24, &narrow, false));
        assert!(text.contains("tk-denoise") && text.contains("Next action"));
    }

    #[test]
    fn full_screen_detail_scrolls_to_every_section() {
        let mut state = UiState {
            detail_open: true,
            ..sample()
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
        assert!(text.contains("more lines"), "{text}");
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
        // o5 is paid with its warranty over: next action "archive".
        let buf = render_with(80, 24, &state, false, Theme::LIGHT);
        let text = all(&buf);
        let (y, row7) = line_of(&text, "tk-denoise").unwrap();
        let y = y as u16;
        assert!(row7.starts_with(" \u{258c}"), "selection marker: {row7}");
        assert_eq!(buf[(1, y)].fg, Theme::LIGHT.accent);
        let x = crate::text::width(&row7[..row7.find("tk-denoise").unwrap()]) as u16;
        assert_eq!(buf[(x, y)].bg, Theme::LIGHT.sel);
        let chip_x = crate::text::width(&row7[..row7.find("delivered").unwrap()]) as u16;
        assert_eq!(buf[(chip_x, y)].fg, Theme::LIGHT.unpaid);
        // The second line reads as part of the row and is not dim.
        let below = text.lines().nth(usize::from(y) + 1).unwrap();
        assert!(below.contains("\u{21b3} 2026-09-20: 预览已发送"), "{below}");
        let sx = crate::text::width(&below[..below.find("预览").unwrap()]) as u16;
        assert_eq!(buf[(sx, y + 1)].fg, Theme::LIGHT.text);
        // Paid in warranty is amber; paid with the warranty over is not.
        let mut paid = 0;
        for r in &state.data.orders {
            if r.order.status == gig_core::models::OrderStatus::Paid {
                paid += 1;
                let (py, l) = line_of(&text, &format!(" {} ", r.order.slug)).unwrap();
                let px = crate::text::width(&l[..l.find("paid").unwrap()]) as u16;
                let want = if r.next_action == "archive" {
                    Theme::LIGHT.text
                } else {
                    Theme::LIGHT.warranty
                };
                assert_eq!(buf[(px, py as u16)].fg, want, "{}", r.order.slug);
            }
        }
        assert_eq!(paid, 2, "one in warranty, one to archive");
        state.view = View::History;
        let buf = render_with(80, 24, &state, true, Theme::LIGHT);
        let text = all(&buf);
        assert!(text.contains("until") && text.contains("4/5"), "{text}");
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
                " p paid  y copy link  n note  k score   \u{b7}   / filter  a archived  N new  ? keys  q quit"
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
            line.contains("1-4 views  T theme  ? keys  q quit"),
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

    #[test]
    fn money_list_says_what_it_leaves_out() {
        let mut state = sample();
        state.view = View::Money;
        for i in 0..20 {
            state.data.money.owed.push(crate::data::money::Owed {
                order_id: 7,
                slug: format!("owed-{i:02}"),
                price_minor: Some(100_000 + i),
                currency: "CNY".into(),
                days: Some(30 - i),
            });
        }
        let text = all(&render(80, 24, &state, false));
        assert!(text.contains("owed-00"), "{text}");
        assert!(text.contains(" more"), "{text}");
        assert!(!text.contains("owed-19"));
        assert!(
            text.contains("received per month, 2025-10 to 2026-09"),
            "{text}"
        );
        // `/` does nothing in Money.
        let slash = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('/'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.handle_key(slash, 80);
        assert!(!state.filter().editing);
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
    }

    #[test]
    fn history_money_and_drafts_render() {
        let mut state = sample();
        state.view = View::History;
        let text = all(&render(200, 50, &state, true));
        assert!(text.contains("4/5"), "scorecard score");
        assert!(line_of(&text, " o8 ").is_some(), "archived in history");

        state.view = View::Money;
        state.data.money.owed.push(crate::data::money::Owed {
            order_id: 7,
            slug: "tk-denoise".into(),
            price_minor: Some(80000),
            currency: "CNY".into(),
            days: Some(59),
        });
        state.data.money.outstanding.gross = 80000;
        state.data.money.outstanding.take_home = 48000;
        if let Some(m) = state.data.money.by_month.last_mut() {
            m.amount.gross = 120000;
        }
        for (w, h) in [(80, 24), (200, 50)] {
            let text = all(&render(w, h, &state, true));
            for s in [
                "outstanding",
                "take-home 480",
                "received per month",
                "1200",
                "09",
            ] {
                assert!(text.contains(s), "{w}x{h}: {s}");
            }
            assert!(line_of(&text, "tk-denoise").is_some_and(|(_, l)| l.contains("59d")));
        }

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
}
