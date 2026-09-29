//! The layout shell: money banner on top, the view body, a key hint line at
//! the bottom. Whitespace and colour instead of borders (spec section 3).

use crate::app::{UiState, View, WIDE_COLUMNS};
use crate::data::money::major;
use crate::icons::Icons;
use crate::theme::Theme;
use crate::{help, popup, views};
use ratatui::layout::{Constraint, Layout, Rect};
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

/// Areas of the shell for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shell {
    pub banner: Rect,
    pub body: Rect,
    /// Right pane for the Orders detail; `None` under 110 columns or in
    /// other views.
    pub detail: Option<Rect>,
    pub hint: Rect,
}

pub fn shell(area: Rect, state: &UiState) -> Shell {
    let [banner, _gap, main, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let wide = area.width >= WIDE_COLUMNS && state.view == View::Orders && !state.detail_open;
    if wide {
        let [body, _gutter, detail] = Layout::horizontal([
            Constraint::Percentage(55),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .areas(main);
        Shell {
            banner,
            body,
            detail: Some(detail),
            hint,
        }
    } else {
        Shell {
            banner,
            body: main,
            detail: None,
            hint,
        }
    }
}

pub fn draw(frame: &mut Frame, cx: &RenderCx) {
    let area = frame.area();
    frame.render_widget(Block::new().style(cx.theme.base()), area);
    let s = shell(area, cx.state);
    draw_banner(frame, s.banner, cx);
    if cx.state.detail_open {
        views::detail::render(frame, s.body, cx);
    } else {
        views::render(frame, s.body, cx.state.view, cx);
    }
    if let Some(detail) = s.detail {
        views::detail::render(frame, detail, cx);
    }
    draw_hint(frame, s.hint, cx);
    if cx.state.help_open {
        help::render(frame, area, cx);
    }
    if let Some(p) = &cx.state.popup {
        popup::render(frame, area, p, cx.theme);
    }
}

/// View tabs on the left, money summary on the right.
fn draw_banner(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let mut tabs = vec![Span::styled(" gig ", t.title())];
    for v in View::ALL {
        let label = cx.icons.label(cx.icons.view(v), v.title());
        let text = format!(" {} {label} ", v.index() + 1);
        let style = if v == cx.state.view {
            t.text().bg(t.selection_bg).add_modifier(Modifier::BOLD)
        } else {
            t.dim()
        };
        tabs.push(Span::styled(text, style));
    }
    let tabs = Line::from(tabs);
    let tabs_width = tabs.width();
    frame.render_widget(Paragraph::new(tabs), area);
    let m = &cx.state.data.money;
    let money = Line::from(vec![
        Span::styled("owed ", t.dim()),
        Span::styled(major(m.outstanding.gross), t.text().fg(t.unpaid)),
        Span::styled("  month ", t.dim()),
        Span::styled(major(m.month.gross), t.text()),
        Span::styled("  year ", t.dim()),
        Span::styled(format!("{} ", major(m.year.gross)), t.text()),
    ])
    .right_aligned();
    if usize::from(area.width) > tabs_width + money.width() {
        frame.render_widget(Paragraph::new(money), area);
    }
}

fn draw_hint(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let filter = cx.state.filter();
    let line = if matches!(cx.state.popup, Some(popup::Popup::Progress { .. })) {
        Line::from(Span::styled(
            " uploading; keys are ignored until it ends",
            t.dim(),
        ))
    } else if cx.state.popup.is_some() {
        // The popup box carries its own key hints; global keys are off.
        Line::from(vec![
            Span::styled(" Esc ", t.key()),
            Span::styled("close popup", t.dim()),
        ])
    } else if filter.editing {
        Line::from(vec![
            Span::styled(" / ", t.key()),
            Span::styled(filter.text.clone(), t.text()),
            Span::styled("_", t.dim()),
            Span::styled("   Enter keep  Esc clear", t.dim()),
        ])
    } else if let Some(err) = &cx.state.error {
        Line::from(Span::styled(format!(" {err}"), t.error()))
    } else {
        let mut spans = Vec::new();
        if !filter.text.is_empty() {
            spans.push(Span::styled(" filter ", t.dim()));
            spans.push(Span::styled(filter.text.clone(), t.key()));
            spans.push(Span::raw(" "));
        }
        let keys: &[(&str, &str)] = if cx.state.detail_open {
            &[("Esc", "back"), ("?", "help"), ("q", "quit")]
        } else {
            &[
                ("1-4", "view"),
                ("Tab", "next"),
                ("/", "filter"),
                ("r", "refresh"),
                ("?", "help"),
                ("q", "quit"),
            ]
        };
        for (k, what) in keys {
            spans.push(Span::styled(format!(" {k} "), t.key()));
            spans.push(Span::styled(format!("{what} "), t.dim()));
        }
        Line::from(spans)
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
        let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
        let theme = Theme::DARK;
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
        assert!(shell(buf.area, &state).detail.is_none());
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
        let detail = s.detail.expect("detail pane at 200 columns");
        assert!(detail.x > s.body.x + s.body.width);
        assert!(all(&buf).contains("no order selected"));
        // Pane layout stays inside the frame.
        assert!(detail.right() <= 200 && s.hint.bottom() <= 50);
    }

    #[test]
    fn banner_shows_snapshot_money() {
        let mut state = UiState::default();
        state.data.money.outstanding.gross = 130000;
        state.data.money.month.gross = 80050;
        state.data.money.year.gross = 1_200_000;
        let top = row(&render(200, 50, &state, true), 0);
        assert!(top.contains("owed 1300"), "{top}");
        assert!(top.contains("month 800.50"), "{top}");
        assert!(top.contains("year 12000"), "{top}");
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
            assert!(shell(buf.area, &state).detail.is_none());
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
            assert!(text.contains("keys"));
            assert!(text.contains("refresh"));
            assert!(text.contains("upload package"));
            assert!(text.contains("\u{256d}"), "rounded corner");
        }
    }

    #[test]
    fn filter_input_shows_on_hint_line() {
        let mut state = UiState::default();
        state.filters[0].editing = true;
        state.filters[0].text = "tk-".into();
        let buf = render(80, 24, &state, false);
        assert!(row(&buf, 23).contains("/ tk-_"));
    }

    #[test]
    fn errors_show_on_hint_line() {
        let state = UiState {
            error: Some("db: locked".into()),
            ..UiState::default()
        };
        let buf = render(80, 24, &state, false);
        assert!(row(&buf, 23).contains("db: locked"));
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
            assert_eq!(c.bg, Theme::DARK.selection_bg);
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
        let buf = render(80, 8, &state, false);
        let text = all(&buf);
        assert!(line_of(&text, " o2 ").is_some(), "{text}");
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
