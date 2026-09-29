//! History view (spec 2.4): every order, archived and cancelled included,
//! newest first, with scorecard score and warranty end. `Enter` opens the
//! detail full screen.

use super::orders::MIN_TITLE;
use super::{
    banded, cell, cell_right, chip_width, empty, price, status_chip, status_style, window_start,
};
use crate::data::{day_part, Group, OrderRow};
use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Column widths of a History row, in display cells. `title` is 0 when
/// there is no room for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Columns {
    pub icon: usize,
    pub slug: usize,
    pub chip: usize,
    /// 10 for `YYYY-MM-DD`, 5 for `MM-DD` under `FULL_DATES` columns.
    pub date: usize,
    pub price: usize,
    pub title: usize,
}

const SCORE: usize = 5;
/// From this width the dates keep their year.
const FULL_DATES: usize = 120;

impl Columns {
    pub fn fit(width: usize, cx: &RenderCx, rows: &[&OrderRow]) -> Self {
        let icon = if cx.icons.enabled { 2 } else { 0 };
        let mut slug = rows
            .iter()
            .map(|r| crate::text::width(&r.order.slug))
            .max()
            .unwrap_or(4)
            .clamp(4, 20);
        let chip = chip_width(cx);
        let date = if width >= FULL_DATES { 10 } else { 5 };
        let price_w = rows
            .iter()
            .map(|r| crate::text::width(&price(r.order.price_minor)))
            .max()
            .unwrap_or(1)
            .clamp(5, 10);
        // " " icon slug _ chip _ created _ score __ warranty _ price __ title
        let mut fixed =
            1 + icon + slug + 1 + chip + 1 + date + 1 + SCORE + 2 + date + 1 + price_w + 2;
        // The slug column gives way first: to 8 cells to keep a title, then
        // to 4 so the row fits.
        let cut = (fixed + MIN_TITLE)
            .saturating_sub(width)
            .min(slug.saturating_sub(8));
        slug -= cut;
        fixed -= cut;
        if fixed > width {
            let cut = (fixed - width).min(slug - 4.min(slug));
            slug -= cut;
            fixed -= cut;
        }
        let mut title = width.saturating_sub(fixed);
        if title < MIN_TITLE {
            title = 0;
        }
        Self {
            icon,
            slug,
            chip,
            date,
            price: price_w,
            title,
        }
    }
}

/// `YYYY-MM-DD`, or `MM-DD` in a 5-cell column.
fn date(d: Option<&str>, cells: usize) -> String {
    match d {
        Some(d) if cells < 10 => d.get(5..10).unwrap_or(d).to_string(),
        Some(d) => d.to_string(),
        None => "-".to_string(),
    }
}

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let rows = cx.state.order_list();
    let c = Columns::fit(usize::from(area.width), cx, &rows);

    let d = t.muted();
    let mut header = vec![
        Span::raw(" ".repeat(1 + c.icon)),
        cell("slug", c.slug, d),
        Span::raw(" "),
        cell("status", c.chip, d),
        Span::raw(" "),
        cell(if c.date < 10 { "added" } else { "created" }, c.date, d),
        Span::raw(" "),
        cell_right("score", SCORE, d),
        Span::raw("  "),
        cell(if c.date < 10 { "until" } else { "warranty" }, c.date, d),
        Span::raw(" "),
        cell_right("price", c.price, d),
    ];
    if c.title > 0 {
        header.push(Span::raw("  "));
        header.push(cell("title", c.title, d));
    }
    let mut lines = vec![Line::from(header)];
    if rows.is_empty() {
        frame.render_widget(Paragraph::new(lines), area);
        let body = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };
        let msg = if cx.state.filter().text.is_empty() {
            "no orders yet"
        } else {
            "no order matches the filter"
        };
        empty(frame, body, t, &[msg]);
        return;
    }
    let selected_id = cx.state.selected_order().map(|r| r.order.id);
    let selected = rows
        .iter()
        .position(|r| Some(r.order.id) == selected_id)
        .unwrap_or(0);
    let heights = vec![1; rows.len()];
    let start = window_start(&heights, selected, area.height.saturating_sub(1));
    for (i, r) in rows.iter().enumerate().skip(start) {
        let line = row_line(cx, r, &c);
        lines.push(if i == selected {
            banded(line, area.width, t)
        } else {
            line
        });
        if lines.len() >= usize::from(area.height) {
            break;
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn row_line(cx: &RenderCx, r: &OrderRow, c: &Columns) -> Line<'static> {
    let t = cx.theme;
    let o = &r.order;
    let text = if r.group == Group::Closed {
        t.muted()
    } else {
        t.text()
    };
    let score = r
        .scorecard
        .as_ref()
        .and_then(|s| s.score)
        .map_or("-".to_string(), |s| format!("{s}/5"));
    let mut spans = vec![Span::raw(" ")];
    if c.icon > 0 {
        spans.push(cell(
            cx.icons.project_type(o.project_type),
            c.icon,
            t.muted(),
        ));
    }
    spans.extend([
        cell(&o.slug, c.slug, text.add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        cell(&status_chip(cx, o.status), c.chip, status_style(cx, r)),
        Span::raw(" "),
        cell(&date(day_part(&o.created_at), c.date), c.date, t.muted()),
        Span::raw(" "),
        cell_right(&score, SCORE, text),
        Span::raw("  "),
        cell(
            &date(o.warranty_until.as_deref(), c.date),
            c.date,
            t.muted(),
        ),
        Span::raw(" "),
        cell_right(&price(o.price_minor), c.price, text),
    ]);
    if c.title > 0 {
        spans.push(Span::raw("  "));
        spans.push(cell(&o.title, c.title, text));
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
            for width in 40..=250usize {
                let c = Columns::fit(width, &cx, &rows);
                if width >= 70 {
                    assert!(c.title >= MIN_TITLE, "title at {width}: {c:?}");
                }
                assert!(c.title == 0 || c.title >= MIN_TITLE);
                if width >= 60 {
                    for r in &rows {
                        let w = row_line(&cx, r, &c).width();
                        assert!(w <= width, "{w} > {width}");
                    }
                }
            }
            // The price column is sized from the data.
            let c = Columns::fit(200, &cx, &rows);
            assert_eq!(c.price, "1234567.89".len());
            assert_eq!(c.date, 10);
            assert_eq!(Columns::fit(80, &cx, &rows).date, 5);
        }
    }
}
