//! History view (TUI-DESIGN.md section 10.2): every order, archived and
//! cancelled included, newest first, flat. `Enter` opens the detail full
//! screen.

use super::orders::{GAP, PRICE, SLUG, STATUS};
use super::{
    banded, cell, cell_right, empty, empty_right, gap, highlighted, marker, price_cell,
    status_chip, status_style, window_start,
};
use crate::data::{day_part, Group, OrderRow};
use crate::ui::{RenderCx, WidthClass, MIN_TITLE};
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const DATE: usize = 10;
const SCORE: usize = 3;

/// Column layout of a History row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Columns {
    pub icon: usize,
    /// Created and warranty-end columns (dropped at Narrow).
    pub dates: bool,
    /// 0 when there is no room for it.
    pub title: usize,
}

impl Columns {
    pub fn fit(width: usize, class: WidthClass, icons: bool) -> Self {
        let mut c = Self {
            icon: if icons { 2 } else { 0 },
            dates: class != WidthClass::Narrow,
            title: 0,
        };
        let title = width.saturating_sub(c.fixed() + GAP);
        c.title = if title >= MIN_TITLE { title } else { 0 };
        c
    }

    /// Cells up to the end of the price column.
    pub fn fixed(&self) -> usize {
        let dates = if self.dates { 2 * (DATE + GAP) } else { 0 };
        2 + self.icon + SLUG + GAP + STATUS + GAP + dates + SCORE + GAP + PRICE
    }
}

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let rows = cx.state.order_list();
    let class = WidthClass::of(frame.area().width);
    let c = Columns::fit(usize::from(area.width), class, cx.icons.enabled);
    let m = t.muted();
    let mut header = vec![
        gap(2 + c.icon),
        cell("order", SLUG, m),
        gap(GAP),
        cell("status", STATUS, m),
    ];
    if c.dates {
        header.push(gap(GAP));
        header.push(cell("created", DATE, m));
    }
    // "score" is wider than its 3-cell column: it takes the gap before it.
    header.push(cell_right("score", GAP + SCORE, m));
    header.push(gap(GAP));
    if c.dates {
        header.push(cell("warranty", DATE, m));
        header.push(gap(GAP));
    }
    header.push(cell_right(cx.state.data.currency(), PRICE, m));
    if c.title > 0 {
        header.push(gap(GAP));
        header.push(Span::styled("title", m));
    }
    let mut lines = vec![Line::from(header), Line::raw("")];
    if rows.is_empty() {
        frame.render_widget(Paragraph::new(lines), area);
        let body = Rect {
            y: area.y + 2,
            height: area.height.saturating_sub(2),
            ..area
        };
        let msg: &[&str] = if cx.state.filter().text.is_empty() {
            &["no orders yet", "N new order"]
        } else {
            &["no order matches the filter", "Esc clears it"]
        };
        empty(frame, body, t, msg);
        return;
    }
    let selected_id = cx.state.selected_order().map(|r| r.order.id);
    let selected = rows
        .iter()
        .position(|r| Some(r.order.id) == selected_id)
        .unwrap_or(0);
    let heights = vec![1; rows.len()];
    let start = window_start(&heights, selected, area.height.saturating_sub(2));
    for (i, r) in rows.iter().enumerate().skip(start) {
        if lines.len() >= usize::from(area.height) {
            break;
        }
        let line = row_line(cx, r, &c, i == selected);
        lines.push(if i == selected {
            banded(line, area.width, t)
        } else {
            line
        });
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn row_line(cx: &RenderCx, r: &OrderRow, c: &Columns, selected: bool) -> Line<'static> {
    let t = cx.theme;
    let o = &r.order;
    let closed = r.group == Group::Closed;
    let ink = if closed {
        t.text().fg(t.archived)
    } else {
        t.text()
    };
    let meta = if closed { ink } else { t.muted() };
    let mut spans: Vec<Span<'static>> = marker(selected, t).into();
    if c.icon > 0 {
        spans.push(Span::styled(cx.icons.project_type(o.project_type), meta));
        spans.push(Span::raw(" "));
    }
    let slug_style = if selected {
        ink.add_modifier(Modifier::BOLD)
    } else {
        ink
    };
    spans.extend(highlighted(
        &o.slug,
        SLUG,
        &cx.state.filter().text,
        slug_style,
    ));
    spans.push(gap(GAP));
    let chip = if closed { ink } else { status_style(cx, r) };
    spans.push(cell(&status_chip(cx, o.status), STATUS, chip));
    spans.push(gap(GAP));
    let date_cell = |d: Option<&str>, style| match d {
        Some(d) => cell(d, DATE, style),
        None => cell(super::DOT, DATE, t.dim()),
    };
    if c.dates {
        spans.push(date_cell(day_part(&o.created_at), meta));
        spans.push(gap(GAP));
    }
    let score = r.scorecard.as_ref().and_then(|s| s.score);
    spans.push(match score {
        Some(s) => cell_right(&format!("{s}/5"), SCORE, ink),
        None => empty_right(SCORE, t),
    });
    spans.push(gap(GAP));
    if c.dates {
        let running = o
            .warranty_until
            .as_deref()
            .is_some_and(|w| w > cx.state.data.today.as_str());
        let style = if closed {
            ink
        } else if running {
            t.text().fg(t.warranty)
        } else {
            t.muted()
        };
        spans.push(date_cell(o.warranty_until.as_deref(), style));
        spans.push(gap(GAP));
    }
    spans.push(price_cell(o.price_minor, PRICE, ink, t));
    if c.title > 0 {
        spans.push(gap(GAP));
        spans.push(cell(&o.title, c.title, ink));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{UiState, View};
    use crate::data::tests::sample_snapshot;
    use crate::icons::Icons;
    use crate::theme::Theme;

    #[test]
    fn rows_fit_and_keep_a_title() {
        let mut data = sample_snapshot("2026-09-29");
        data.orders[0].order.title = "图像去噪与超分辨率批处理工具开发".repeat(4);
        data.orders[3].order.slug = "a-rather-long-slug-for-a-row".into();
        data.orders[2].order.price_minor = Some(123_456_789);
        let state = UiState {
            data,
            view: View::History,
            ..UiState::default()
        };
        for icons in [true, false] {
            let icons = Icons::new(icons);
            let cx = RenderCx {
                state: &state,
                theme: &Theme::DARK,
                icons: &icons,
            };
            let rows = state.order_list();
            for width in 58..=250usize {
                let class = WidthClass::of(width as u16 + 2);
                let c = Columns::fit(width, class, icons.enabled);
                assert!(c.title == 0 || c.title >= MIN_TITLE);
                if width >= 78 {
                    assert!(c.title >= MIN_TITLE, "title at {width}: {c:?}");
                }
                for r in &rows {
                    let w = row_line(&cx, r, &c, true).width();
                    assert!(w <= width.max(c.fixed()), "{w} > {width}");
                }
            }
            assert!(!Columns::fit(78, WidthClass::Narrow, true).dates);
            assert!(Columns::fit(198, WidthClass::Wide, true).dates);
        }
    }
}
