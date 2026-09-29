//! Orders view (spec 2.1): the active orders, one line each, the selected
//! one expanded to two lines with the latest JOB.md status entry.

use super::{
    banded, cell, cell_right, chip_width, days, empty, price, status_chip, status_style,
    window_start,
};
use crate::data::{Group, OrderRow};
use crate::text;
use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Column widths of one row, in display cells. `title` is 0 when the
/// terminal is too narrow for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Columns {
    pub icon: usize,
    pub slug: usize,
    pub chip: usize,
    pub next: usize,
    pub days: usize,
    pub price: usize,
    pub title: usize,
}

/// Cells between columns.
const GAP: usize = 1;
/// Cells between the price and the title.
const TITLE_GAP: usize = 2;
/// Narrowest title worth showing.
pub(crate) const MIN_TITLE: usize = 6;
/// Narrowest next action kept when the slug column shrinks.
const MIN_NEXT: usize = 8;

impl Columns {
    pub fn fit(width: usize, cx: &RenderCx, rows: &[&OrderRow]) -> Self {
        let icon = if cx.icons.enabled { 2 } else { 0 };
        let longest = rows
            .iter()
            .map(|r| text::width(&r.order.slug))
            .max()
            .unwrap_or(4);
        let mut slug = longest.clamp(4, 20);
        let chip = chip_width(cx);
        let days = 4;
        let price = rows
            .iter()
            .map(|r| text::width(&price(r.order.price_minor)))
            .max()
            .unwrap_or(1)
            .clamp(5, 10);
        // Leading space, gaps before chip, next, days and price, and two
        // cells before the title so a right-aligned price does not run
        // into it.
        let mut fixed = 1 + icon + slug + chip + days + price + 4 * GAP + TITLE_GAP;
        // Narrow: the slug column gives way first, down to 8 cells to keep a
        // next action and a title, then down to 4.
        let want = MIN_NEXT + MIN_TITLE;
        let cut = (fixed + want)
            .saturating_sub(width)
            .min(slug.saturating_sub(8));
        slug -= cut;
        fixed -= cut;
        if fixed > width {
            let cut = (fixed - width).min(slug - 4.min(slug));
            slug -= cut;
            fixed -= cut;
        }
        let rest = width.saturating_sub(fixed);
        // The next action is what the row is for: at least 15 cells when
        // there is room (26 fits "warranty until 2026-10-10"); the title
        // takes the rest.
        let mut next = (rest * 45 / 100).clamp(15.min(rest), 26);
        let mut title = rest - next;
        if title < MIN_TITLE {
            if rest >= MIN_NEXT + MIN_TITLE {
                title = MIN_TITLE;
                next = rest - MIN_TITLE;
            } else {
                next = rest;
                title = 0;
            }
        }
        Self {
            icon,
            slug,
            chip,
            next,
            days,
            price,
            title,
        }
    }

    /// Cells before the slug column (the second line starts there).
    pub fn indent(&self) -> usize {
        1 + self.icon
    }
}

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let rows = cx.state.order_list();
    let cols = Columns::fit(usize::from(area.width), cx, &rows);
    let mut lines = vec![header(cx, &cols)];
    if rows.is_empty() {
        let filter = &cx.state.filter().text;
        let msg: &[&str] = if !filter.is_empty() {
            &["no order matches the filter", "Esc clears it"]
        } else if cx.state.show_closed {
            &["no orders yet", "N registers one"]
        } else {
            &["no active orders", "N registers one, a shows archived ones"]
        };
        frame.render_widget(Paragraph::new(lines), area);
        let body = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };
        empty(frame, body, t, msg);
        return;
    }
    let selected_id = cx.state.selected_order().map(|r| r.order.id);
    let selected = rows
        .iter()
        .position(|r| Some(r.order.id) == selected_id)
        .unwrap_or(0);
    let heights: Vec<u16> = (0..rows.len())
        .map(|i| if i == selected { 2 } else { 1 })
        .collect();
    let start = window_start(&heights, selected, area.height.saturating_sub(1));
    for (i, r) in rows.iter().enumerate().skip(start) {
        let line = row_line(cx, &cols, r);
        if i == selected {
            lines.push(banded(line, area.width, t));
            lines.push(banded(status_line(cx, &cols, r, area.width), area.width, t));
        } else {
            lines.push(line);
        }
        if lines.len() >= usize::from(area.height) {
            break;
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn header(cx: &RenderCx, c: &Columns) -> Line<'static> {
    let d = cx.theme.muted();
    let mut spans = vec![
        Span::raw(" ".repeat(c.indent())),
        cell("slug", c.slug, d),
        Span::raw(" "),
        cell("status", c.chip, d),
        Span::raw(" "),
        cell("next", c.next, d),
        Span::raw(" "),
        cell_right("days", c.days, d),
        Span::raw(" "),
        cell_right("price", c.price, d),
    ];
    if c.title > 0 {
        spans.push(Span::raw("  "));
        spans.push(cell("title", c.title, d));
    }
    Line::from(spans)
}

fn row_line(cx: &RenderCx, c: &Columns, r: &OrderRow) -> Line<'static> {
    let t = cx.theme;
    let o = &r.order;
    let closed = r.group == Group::Closed;
    let text = if closed { t.muted() } else { t.text() };
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
        cell(&r.next_action, c.next, text),
        Span::raw(" "),
        cell_right(&days(r.days_in_status), c.days, t.muted()),
        Span::raw(" "),
        cell_right(&price(o.price_minor), c.price, text),
    ]);
    if c.title > 0 {
        spans.push(Span::raw("  "));
        spans.push(cell(&o.title, c.title, text));
    }
    Line::from(spans)
}

/// Second line of the selected row: the newest JOB.md status entry.
fn status_line(cx: &RenderCx, c: &Columns, r: &OrderRow, width: u16) -> Line<'static> {
    let t = cx.theme;
    let indent = c.indent();
    let lead = "\u{21b3} ";
    let room = usize::from(width).saturating_sub(indent + text::width(lead) + 1);
    // Drawn on the selection band, so the status is in the text colour
    // (dim would be the least legible text on screen).
    let (text_, style) = match r.job.latest_status() {
        Some(s) => (s.to_string(), t.text().add_modifier(Modifier::ITALIC)),
        None if r.order.dev_path.is_none() => ("(no project directory)".to_string(), t.muted()),
        None if !r.job.found => ("(no JOB.md)".to_string(), t.muted()),
        None => ("(no status entry in JOB.md)".to_string(), t.muted()),
    };
    Line::from(vec![
        Span::raw(" ".repeat(indent)),
        Span::styled(lead, t.muted()),
        Span::styled(text::truncate(&text_, room), style),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::UiState;
    use crate::data::tests::sample_snapshot;
    use crate::icons::Icons;
    use crate::theme::Theme;

    #[test]
    fn rows_never_exceed_the_width() {
        let mut data = sample_snapshot("2026-09-29");
        data.orders[0].order.title = "图像去噪与超分辨率批处理工具开发".repeat(4);
        // orders[3] is id 7, delivered, so it is in the active list.
        data.orders[3].order.slug = "a-rather-long-slug-for-a-row".into();
        let state = UiState {
            data,
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
                let cols = Columns::fit(width, &cx, &rows);
                if width >= 80 {
                    assert!(cols.title >= MIN_TITLE, "title shown at {width}");
                }
                if width >= 60 {
                    // The detail pane leaves 60 columns at 110.
                    assert!(cols.title >= MIN_TITLE, "title shown at {width}");
                }
                if width >= 200 {
                    assert!(cols.next >= 25, "a full warranty date at {width}");
                }
                for r in &rows {
                    let w = row_line(&cx, &cols, r).width();
                    assert!(w <= width, "{w} > {width}");
                    let s = status_line(&cx, &cols, r, width as u16).width();
                    assert!(s <= width, "status line {s} > {width}");
                }
                assert!(header(&cx, &cols).width() <= width);
            }
        }
    }
}
