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

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect()
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
        assert!(text.contains("Orders"));
        assert!(!text.contains("Detail"));
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
        assert!(all(&buf).contains("Detail"));
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
        for v in [View::Drafts, View::Money, View::History] {
            let state = UiState {
                view: v,
                ..UiState::default()
            };
            let buf = render(200, 50, &state, true);
            assert!(shell(buf.area, &state).detail.is_none());
            assert!(row(&buf, 2).contains(v.title()));
        }
    }

    #[test]
    fn narrow_enter_shows_detail_full_screen() {
        let state = UiState {
            detail_open: true,
            ..UiState::default()
        };
        let buf = render(80, 24, &state, false);
        assert!(row(&buf, 2).contains("Detail"));
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
}
